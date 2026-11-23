//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A repo's version timeline, read off the commits of its trunks that touch
//! the manifest, and the tags measured against it: which tag released which
//! version change, which changes no tag released, and which tags a history
//! rewrite left off every trunk, with where each belongs.

use std::collections::BTreeMap;
use std::path::Path;

use homma_api::Version;

use super::git::{self, GitError};
use super::packages::{self, tag_version};
use super::sh;

/// One change of the version, as a commit made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The commit's sha, nine characters.
    pub sha:     String,
    /// Its author date, `YYYY-MM-DD`.
    pub date:    String,
    pub subject: String,
    /// The version before, `None` for the first one recorded.
    pub from:    Option<String>,
    pub to:      String,
}

/// The `N` of a design-phase marker, `0.0.0-dNN`.
pub fn design_round(v: &str) -> Option<u32> {
    let n = v.trim().strip_prefix("0.0.0-d")?;
    if n.is_empty() || !n.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}

/// The `x.y.z` of a version, its pre-release and build left off.
fn core(v: &str) -> Option<Version> {
    let c = v.split(['-', '+']).next()?;
    let parts: Vec<&str> = c.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(Version::new(
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Whether `to` is exactly one legal step past `from`, and why or why not.
pub fn sensible(from: Option<&str>, to: &str) -> (bool, String) {
    let Some(from) = from else {
        return (true, "first version recorded".into());
    };
    match (design_round(from), design_round(to)) {
        (Some(df), Some(dt)) => {
            return if dt == df + 1 {
                (true, format!("design round d{df:02} to d{dt:02}"))
            } else if dt <= df {
                (
                    false,
                    format!("design round went backwards, d{df:02} to d{dt:02}"),
                )
            } else {
                (
                    false,
                    format!(
                        "design rounds skip: d{df:02} continues to d{:02}, not d{dt:02}",
                        df + 1
                    ),
                )
            };
        },
        (Some(df), None) => {
            return if to == "0.0.1" {
                (
                    true,
                    format!("leaves the design phase at d{df:02} for {to}"),
                )
            } else {
                (
                    false,
                    format!("the first release after a design round is 0.0.1, not {to}"),
                )
            };
        },
        (None, Some(_)) => {
            return (
                false,
                format!("went from a real version {from} back into the design phase at {to}"),
            );
        },
        (None, None) => {},
    }
    let (Some(a), Some(b)) = (core(from), core(to)) else {
        return (false, "not a plain x.y.z on one side".into());
    };
    if a == b {
        return (true, "unchanged".into());
    }
    if b < a {
        return (false, format!("moved backwards, {from} to {to}"));
    }
    if a.is_one_step(&b) {
        let kind = if b.major != a.major {
            "major"
        } else if b.minor != a.minor {
            "minor"
        } else {
            "patch"
        };
        return (true, kind.into());
    }
    (
        false,
        format!(
            "skips: {from} continues to {}.{}.{} / {}.{}.0 / {}.0.0, not {to}",
            a.major,
            a.minor,
            a.patch + 1,
            a.major,
            a.minor + 1,
            a.major + 1
        ),
    )
}

/// The refs a release tag belongs on: the trunk and the release line, here
/// and on `origin`, those that exist; `HEAD` where none does.
pub fn mainlines(root: &Path, trunks: &[&str]) -> Vec<String> {
    let mut cands: Vec<String> = trunks.iter().map(|t| t.to_string()).collect();
    cands.extend(trunks.iter().map(|t| format!("origin/{t}")));
    let mut found: Vec<String> = Vec::new();
    for c in cands {
        if !found.contains(&c) && git::sha(root, &c).is_ok() {
            found.push(c);
        }
    }
    if found.is_empty() {
        found.push("HEAD".into());
    }
    found
}

/// The version a manifest's text carries: a `version = "..."` line in a toml
/// one, the `version` field in a json one.
pub fn version_in(text: &str, manifest: &str) -> Option<String> {
    if manifest.ends_with(".toml") {
        return text.lines().find_map(|l| {
            let rest = l.trim_start().strip_prefix("version")?.trim_start();
            let rest = rest.strip_prefix('=')?.trim_start().strip_prefix('"')?;
            Some(rest.split('"').next()?.to_string())
        });
    }
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    doc.get("version")?.as_str().map(str::to_string)
}

/// Every version the trunks have carried in `manifest`, oldest first, by
/// ancestry and never by the clock.
pub fn timeline(root: &Path, manifest: &str, trunks: &[&str]) -> Result<Vec<Step>, GitError> {
    let lines = mainlines(root, trunks);
    let mut args: Vec<&str> =
        vec!["log", "--reverse", "--topo-order", "--format=%H%x1f%ad%x1f%s", "--date=short"];
    args.extend(lines.iter().map(String::as_str));
    args.extend(["--", manifest]);
    let log = git::trimmed(root, &args)?;
    let mut steps = Vec::new();
    let mut seen: Option<String> = None;
    for line in log.lines().filter(|l| !l.trim().is_empty()) {
        let mut f = line.split('\x1f');
        let (sha, date, subject) = (
            f.next().unwrap_or(""),
            f.next().unwrap_or(""),
            f.next().unwrap_or(""),
        );
        let Some(v) = git::show(root, sha, manifest)?.and_then(|t| version_in(&t, manifest)) else {
            continue;
        };
        if seen.as_deref() == Some(v.as_str()) {
            continue;
        }
        steps.push(Step {
            sha:     sha.chars().take(9).collect(),
            date:    date.to_string(),
            subject: subject.chars().take(56).collect(),
            from:    seen.clone(),
            to:      v.clone(),
        });
        seen = Some(v);
    }
    Ok(steps)
}

/// The steps a tag may stand for: never `0.0.0`, the placeholder, and
/// nothing at or before a step backwards, since a version cannot move back
/// once it shipped.
pub fn releasable(steps: &[Step]) -> Vec<Step> {
    let mut last_back: Option<usize> = None;
    for (i, s) in steps.iter().enumerate() {
        if let (Some(a), Some(b)) = (s.from.as_deref().and_then(core), core(&s.to)) {
            if !sensible(s.from.as_deref(), &s.to).0 && b < a {
                last_back = Some(i);
            }
        }
    }
    steps
        .iter()
        .enumerate()
        .filter(|(i, s)| last_back.is_none_or(|b| *i > b) && s.to != "0.0.0")
        .map(|(_, s)| s.clone())
        .collect()
}

/// One tag: what it points at, and whether a trunk reaches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name:     String,
    /// The commit, nine characters.
    pub sha:      String,
    /// The commit's tree, in full.
    pub tree:     String,
    pub on_trunk: bool,
    /// Every branch that reaches it, so a tag on a side branch shows as one.
    pub held_by:  Vec<String>,
}

/// Every tag, with its tree and reach.
pub fn all_tags(root: &Path, trunks: &[&str]) -> Result<Vec<Tag>, GitError> {
    let lines = mainlines(root, trunks);
    let mut out = Vec::new();
    for name in git::trimmed(root, &[
        "for-each-ref",
        "--format=%(refname:short)",
        "refs/tags",
    ])?
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty())
    {
        let Ok(sha) = git::sha(root, name) else { continue };
        let tree = git::trimmed(root, &["rev-parse", &format!("{sha}^{{tree}}")])?;
        let mut on_trunk = false;
        for l in &lines {
            if git::is_ancestor(root, &sha, l)? {
                on_trunk = true;
                break;
            }
        }
        let held_by = git::trimmed(root, &["branch", "-a", "--contains", &sha])?
            .lines()
            .map(|b| b.trim().trim_start_matches("* ").to_string())
            .filter(|b| !b.is_empty())
            .collect();
        out.push(Tag {
            name: name.to_string(),
            sha: sha[.. 9].to_string(),
            tree,
            on_trunk,
            held_by,
        });
    }
    Ok(out)
}

/// The tags on each commit, by nine-character sha.
pub fn tags_by_commit(
    root: &Path,
    trunks: &[&str],
) -> Result<BTreeMap<String, Vec<String>>, GitError> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in all_tags(root, trunks)? {
        out.entry(t.sha).or_default().push(t.name);
    }
    Ok(out)
}

/// Which version tag released each step: a tag counts for the last change
/// it reaches, decided by ancestry and never by the order `steps` came in,
/// since the release tags the merge and not the bump.
pub fn released_at(
    root: &Path,
    steps: &[Step],
    trunks: &[&str],
) -> Result<BTreeMap<String, Vec<String>>, GitError> {
    let mut out: BTreeMap<String, Vec<String>> =
        steps.iter().map(|s| (s.sha.clone(), Vec::new())).collect();
    for t in all_tags(root, trunks)? {
        if tag_version(&t.name).is_none() {
            continue;
        }
        let mut latest: Option<&Step> = None;
        for s in steps {
            if !git::is_ancestor(root, &s.sha, &t.sha)? {
                continue;
            }
            if latest.is_none() || git::is_ancestor(root, &latest.unwrap().sha, &s.sha)? {
                latest = Some(s);
            }
        }
        if let Some(s) = latest {
            out.entry(s.sha.clone()).or_default().push(t.name.clone());
        }
    }
    Ok(out)
}

/// How an orphaned tag's place was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matched {
    /// A trunk commit with the same tree: proof.
    Tree,
    /// A trunk commit with the same author date and subject: evidence.
    Metadata,
    /// Nothing.
    Nothing,
}

/// A tag off every trunk, and the trunk commits it could move to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub tag:        String,
    pub old_sha:    String,
    pub tree:       String,
    /// Nine-character shas.
    pub candidates: Vec<String>,
    /// The branches still reaching the old commit; none where a rewrite
    /// dropped it.
    pub held_by:    Vec<String>,
    pub matched:    Matched,
}

impl Orphan {
    /// It lives on a branch that is no trunk, so it is not orphaned at all.
    pub fn side_branch_only(&self) -> bool {
        !self.held_by.is_empty()
    }

    pub fn recoverable(&self) -> bool {
        !self.candidates.is_empty()
    }
}

/// Every tag no trunk reaches, with where each could go: a trunk commit with
/// the same tree first, and only where none has it, one with the same author
/// date and subject.
pub fn orphans(root: &Path, trunks: &[&str]) -> Result<Vec<Orphan>, GitError> {
    let lines = mainlines(root, trunks);
    let index = |format: &str| -> Result<BTreeMap<String, Vec<String>>, GitError> {
        let mut args = vec!["log".to_string(), format!("--format={format}")];
        args.extend(lines.iter().cloned());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut idx: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for line in git::trimmed(root, &args)?.lines() {
            if let Some((sha, key)) = line.split_once('\x1f') {
                idx.entry(key.to_string())
                    .or_default()
                    .push(sha.to_string());
            }
        }
        Ok(idx)
    };
    let trees = index("%H%x1f%T")?;
    let metas = index("%H%x1f%aI%x09%s")?;
    let mut found = Vec::new();
    for t in all_tags(root, trunks)? {
        if t.on_trunk {
            continue;
        }
        let short =
            |v: &Vec<String>| -> Vec<String> { v.iter().map(|c| c[.. 9].to_string()).collect() };
        let (candidates, matched) = match trees.get(&t.tree) {
            Some(c) => (short(c), Matched::Tree),
            None => {
                let meta = git::trimmed(root, &["log", "-1", "--format=%aI%x09%s", &t.sha])?;
                match metas.get(&meta) {
                    Some(c) => (short(c), Matched::Metadata),
                    None => (Vec::new(), Matched::Nothing),
                }
            },
        };
        found.push(Orphan {
            tag: t.name,
            old_sha: t.sha,
            tree: t.tree[.. 9].to_string(),
            candidates,
            held_by: t.held_by,
            matched,
        });
    }
    Ok(found)
}

/// Why a tag was not moved.
#[derive(Debug)]
pub enum Refused {
    NoMatch,
    Ambiguous(usize),
    Git(GitError),
    Journal(std::io::Error),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NoMatch => f.write_str("no commit on a trunk carries that tree"),
            Refused::Ambiguous(n) => write!(f, "{n} commits match; refusing to guess"),
            Refused::Git(e) => write!(f, "{e}"),
            Refused::Journal(e) => write!(f, "the undo line could not be written: {e}"),
        }
    }
}

/// The file in the repository's git directory each tag move is journalled
/// to, as the command that undoes it.
pub const JOURNAL: &str = "retag-journal.txt";

/// Move an orphaned tag onto its one candidate, keeping its message, after
/// appending the undo line to the journal, since a tag move leaves no
/// reflog. `now` stamps the line. Returns the commit it now points at.
pub fn retarget(root: &Path, orphan: &Orphan, now: &str) -> Result<String, Refused> {
    let target = match orphan.candidates.as_slice() {
        [] => return Err(Refused::NoMatch),
        [one] => one.clone(),
        many => return Err(Refused::Ambiguous(many.len())),
    };
    let journal =
        git::trimmed(root, &["rev-parse", "--git-path", JOURNAL]).map_err(Refused::Git)?;
    let journal = root.join(journal);
    {
        use std::io::Write;
        let mut fh = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal)
            .map_err(Refused::Journal)?;
        writeln!(
            fh,
            "git tag -f {} {}   # moved to {target} on {now}",
            orphan.tag, orphan.old_sha
        )
        .map_err(Refused::Journal)?;
    }
    let msg = git::trimmed(root, &["tag", "-l", "--format=%(contents)", &orphan.tag])
        .map_err(Refused::Git)?;
    let msg = if msg.is_empty() {
        format!(
            "{}\n\nRe-pointed after a history rewrite, by matching tree.",
            orphan.tag
        )
    } else {
        msg
    };
    let out = sh::run(root, "git", &[
        "tag",
        "-f",
        "-a",
        &orphan.tag,
        &target,
        "-m",
        &msg,
    ])
    .map_err(|e| Refused::Git(e.into()))?;
    if !out.ok() {
        return Err(Refused::Git(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        }));
    }
    Ok(target)
}

/// Which manifest carries the repo's version: the one named, else the root
/// `Cargo.toml` where it names a version, else a virtual workspace's one
/// versioned member, else `deno.json`. Several versioned members is an
/// error naming them, never a pick.
pub fn manifest_for(root: &Path, named: Option<&str>) -> Result<Option<String>, Vec<String>> {
    if let Some(n) = named {
        return Ok(Some(n.to_string()));
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
        if version_in(&text, "Cargo.toml").is_some() {
            return Ok(Some("Cargo.toml".into()));
        }
        let members = versioned_members(root);
        return match members.len() {
            0 => Ok(None),
            1 => Ok(members.into_iter().next()),
            _ => Err(members),
        };
    }
    if root.join("deno.json").is_file() {
        return Ok(Some("deno.json".into()));
    }
    Ok(None)
}

/// The members carrying a version of their own; one inheriting the root's
/// is the root's line and not one of its own.
pub fn versioned_members(root: &Path) -> Vec<String> {
    packages::member_manifests(root)
        .into_iter()
        .filter(|m| {
            std::fs::read_to_string(root.join(m))
                .ok()
                .and_then(|t| version_in(&t, m))
                .is_some()
        })
        .collect()
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
