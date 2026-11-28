//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The rest of the check catalogue, reported at warn and holding nothing
//! up: the branches and the remote, the tags outside the version scheme, the
//! history scans, what a reader lands on, and the sequence rows over tags
//! and registries. Each row keeps the id the catalogue gave it.

use std::path::Path;

use homma_api::{CheckSeverity, Finding, Version};

use super::check::{Published, tag_version};
use super::git::{self, GitError};
use super::{history, sequence, sh};

/// What the rows read beyond the tree itself.
pub struct Seen<'a> {
    pub root:      &'a Path,
    pub remote:    &'a str,
    pub trunk:     &'a str,
    pub release:   &'a str,
    pub tags:      &'a [String],
    /// The version tags, in version order.
    pub versioned: &'a [(String, Version)],
    /// Every path tracked at `HEAD`.
    pub tracked:   &'a [String],
    pub published: &'a Published,
    /// Package names whose versions are not the tags' (a virtual workspace's
    /// riding members, and members on lines of their own), left out of the
    /// tag-against-registry rows; an own line's tag is no stray tag.
    pub riders:    &'a [String],
    /// The manifest the version lives in, where one does.
    pub manifest:  Option<&'a str>,
}

/// A git call's trimmed stdout, or `None` where git exits non-zero, which a
/// read like `config --get` uses to say "unset".
fn maybe(root: &Path, args: &[&str]) -> Result<Option<String>, GitError> {
    let out = sh::run(root, "git", args)?;
    Ok(out.ok().then(|| out.stdout.trim().to_string()))
}

fn lines(s: Option<String>) -> Vec<String> {
    s.unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The token shapes a tracked file must never carry: GitHub's classic and
/// fine-grained tokens and Slack's.
pub const SECRET_PATTERN: &str =
    "gh[posu]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}";

/// A blob over this many bytes is reported.
pub const LARGE_BLOB: u64 = 1_000_000;

/// Every warn row, in catalogue order.
pub fn findings(s: &Seen<'_>) -> Result<Vec<Finding>, GitError> {
    let mut out = Vec::new();
    let mut warn = |id: &str, msg: String| out.push(Finding::new(id, CheckSeverity::Warn, msg));
    let root = s.root;

    // the working tree and branches
    if let Some(b) = git::current_branch(root)? {
        let upstream = format!("{}/{b}", s.remote);
        if git::sha(root, &upstream).is_ok() {
            let n = git::trimmed(root, &["rev-list", "--count", &format!("HEAD..{upstream}")])?;
            if n != "0" {
                warn(
                    "tree.uptodate",
                    format!("{n} commit(s) on `{upstream}` are not here"),
                );
            }
        }
    }
    let stashes = lines(maybe(root, &["stash", "list"])?).len();
    if stashes > 0 {
        warn(
            "tree.stash",
            format!("{stashes} stash entr(ies) nobody applied"),
        );
    }
    for w in lines(maybe(root, &["worktree", "list", "--porcelain"])?)
        .iter()
        .filter_map(|l| l.strip_prefix("worktree "))
        .skip(1)
    {
        warn(
            "tree.worktrees",
            format!("a worktree is still checked out at {w}"),
        );
    }
    if s.trunk == s.release {
        warn(
            "branch.trunk",
            format!(
                "the working trunk is `{}` itself, no branch apart from it",
                s.release
            ),
        );
    }
    let trunk_ref = format!("{}/{}", s.remote, s.trunk);
    if git::sha(root, &trunk_ref).is_ok() {
        let keep = |b: &str| {
            b.contains('/')
                && !b.ends_with("/HEAD")
                && b != trunk_ref
                && b != format!("{}/{}", s.remote, s.release)
        };
        let merged: Vec<String> = lines(maybe(root, &["branch", "-r", "--merged", &trunk_ref])?)
            .into_iter()
            .filter(|b| !b.contains("->") && keep(b))
            .collect();
        for b in &merged {
            warn(
                "branch.merged",
                format!("`{b}` is merged and still on the remote"),
            );
        }
        let remote = lines(maybe(root, &[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/remotes/{}", s.remote),
        ])?);
        for b in remote.iter().filter(|b| keep(b) && !merged.contains(b)) {
            let n = git::trimmed(root, &["rev-list", "--count", &format!("{trunk_ref}..{b}")])?;
            let when = git::trimmed(root, &["log", "-1", "--format=%cr", b])?;
            warn(
                "branch.unmerged",
                format!("`{b}` has {n} unmerged commit(s), last touched {when}"),
            );
        }
    }
    for line in lines(maybe(root, &[
        "for-each-ref",
        "--format=%(refname:short) %(upstream:track)",
        "refs/heads",
    ])?) {
        if let Some(b) = line.strip_suffix(" [gone]") {
            warn(
                "branch.orphan",
                format!("`{b}` tracks an upstream that is gone"),
            );
        }
    }
    if let Some(url) = maybe(root, &[
        "config",
        "--get",
        &format!("remote.{}.pushurl", s.remote),
    ])? {
        warn(
            "remote.pushurl",
            format!("`{}` has a pushurl set: {}", s.remote, masked(&url)),
        );
    }

    // tags and refs
    for t in s
        .tags
        .iter()
        .filter(|t| tag_version(t).is_none() && !super::lines::is_own_tag(s.riders, t))
    {
        warn("tag.stray", format!("`{t}` is not a version tag"));
    }
    if let Some(m) = s.manifest {
        let steps = history::timeline(root, m, &[s.trunk, s.release])?;
        let mut seen: Vec<&str> = Vec::new();
        for st in &steps {
            if seen.contains(&st.to.as_str()) {
                continue;
            }
            let at: Vec<&str> = steps
                .iter()
                .filter(|x| x.to == st.to)
                .map(|x| x.sha.as_str())
                .collect();
            if at.len() > 1 {
                warn(
                    "tag.bump.once",
                    format!(
                        "{} was set at {} commits: {}",
                        st.to,
                        at.len(),
                        at.join(", ")
                    ),
                );
            }
            seen.push(&st.to);
        }
    }
    let mut dated: Vec<(String, Version)> = Vec::new();
    for (t, v) in s.versioned {
        let date = git::trimmed(root, &[
            "log",
            "-1",
            "--format=%cI",
            &format!("{t}^{{commit}}"),
        ])?;
        dated.push((date, v.clone()));
    }
    dated.sort();
    let order: Vec<Version> = dated.into_iter().map(|(_, v)| v).collect();
    if let Some(i) = sequence::first_descent(&order) {
        warn(
            "tag.order.matches",
            format!(
                "{} was committed after the higher {}",
                order[i],
                order[i - 1]
            ),
        );
    }
    for r in lines(maybe(root, &[
        "for-each-ref",
        "--format=%(refname)",
        "refs/original",
        "refs/replace",
    ])?) {
        warn(
            "ref.filterrepo",
            format!("`{r}` is rewrite plumbing left behind"),
        );
    }

    // history
    let count = |args: &[&str]| -> Result<usize, GitError> { Ok(lines(maybe(root, args)?).len()) };
    let n = count(&["log", "--all", "--oneline", "--", ".github/workflows"])?;
    if n > 0 {
        warn(
            "hist.workflow.past",
            format!("{n} commit(s) touched a workflow file"),
        );
    }
    for hit in lines(maybe(root, &[
        "grep",
        "-I",
        "-l",
        "-E",
        SECRET_PATTERN,
        "HEAD",
    ])?) {
        let path = hit.strip_prefix("HEAD:").unwrap_or(&hit);
        warn(
            "hist.secret.tree",
            format!("`{path}` carries a token-shaped string"),
        );
    }
    let n = count(&["log", "--all", "--oneline", "--", ".env", "secrets/"])?;
    if n > 0 {
        warn(
            "hist.env",
            format!("{n} commit(s) touched `.env` or `secrets/`"),
        );
    }
    for line in lines(maybe(root, &["ls-tree", "-r", "-l", "HEAD"])?) {
        // `<mode> <type> <object> <size>\t<path>`, the size padded
        let (head, path) = line.split_once('\t').unwrap_or((&line, ""));
        let size = head
            .split_whitespace()
            .nth(3)
            .and_then(|p| p.parse::<u64>().ok());
        if let Some(size) = size.filter(|n| *n > LARGE_BLOB) {
            warn("hist.large", format!("`{path}` is {size} bytes"));
        }
    }
    for hit in lines(maybe(root, &["grep", "-I", "-l", "-E", "^<{7} ", "HEAD"])?) {
        let path = hit.strip_prefix("HEAD:").unwrap_or(&hit);
        if git::show(root, "HEAD", path)?.is_some_and(|t| has_conflict(&t)) {
            warn(
                "hist.conflict",
                format!("`{path}` carries conflict markers"),
            );
        }
    }
    let n = count(&["log", "--all", "--oneline", "--grep=Generated with"])?;
    if n > 0 {
        warn(
            "hist.byline",
            format!("{n} commit message(s) carry a generated-with byline"),
        );
    }

    // what a reader lands on
    let top = |p: &&String| !p.contains('/');
    if !s
        .tracked
        .iter()
        .filter(top)
        .any(|p| p.to_ascii_lowercase().starts_with("readme"))
    {
        warn(
            "doc.readme.exists",
            "no README is tracked at the root".into(),
        );
    }
    if !s
        .tracked
        .iter()
        .filter(top)
        .any(|p| p.to_ascii_lowercase().starts_with("licen"))
    {
        warn(
            "doc.licence.file",
            "no LICENSE file is tracked at the root".into(),
        );
    }
    for hit in lines(maybe(root, &[
        "grep", "-c", "\u{2014}", "HEAD", "--", "*.md",
    ])?) {
        let hit = hit.strip_prefix("HEAD:").unwrap_or(&hit);
        if let Some((path, n)) = hit.rsplit_once(':') {
            warn("doc.emdash", format!("`{path}` carries {n} em-dash(es)"));
        }
    }

    // the sequence, over tags and registries
    for t in s.tags {
        let looks = t
            .strip_prefix('v')
            .unwrap_or(t)
            .starts_with(|c: char| c.is_ascii_digit());
        if looks && tag_version(t).is_none() {
            warn(
                "semver.parses",
                format!("`{t}` reads like a version and is not one"),
            );
        }
    }
    let tagged: Vec<Version> = s.versioned.iter().map(|(_, v)| v.clone()).collect();
    for (low, high) in sequence::gaps(&tagged) {
        warn("semver.gaps.tags", format!("tags jump {low} to {high}"));
    }
    for ((reg, name), live) in &s.published.versions {
        if s.riders.contains(name) {
            continue;
        }
        for (low, high) in sequence::gaps(live) {
            if sequence::bridge(&low, &high, &tagged).is_none() {
                warn(
                    "semver.gaps.closable",
                    format!("{name} on {reg}: {low} to {high} has no tags to close it"),
                );
            }
        }
        let mut taken = live.clone();
        taken.extend(
            s.published
                .yanked
                .get(&(*reg, name.clone()))
                .cloned()
                .unwrap_or_default(),
        );
        for v in sequence::sandwiched(&tagged, &taken) {
            warn(
                "reg.sandwiched",
                format!("{name} {v} is tagged, not on {reg}, and below what is"),
            );
        }
    }
    Ok(out)
}

/// A url with whatever sits between `//` and `@` hidden.
pub fn masked(url: &str) -> String {
    match (url.find("//"), url.find('@')) {
        (Some(a), Some(b)) if a + 2 <= b => format!("{}***{}", &url[.. a + 2], &url[b ..]),
        _ => url.to_string(),
    }
}

/// Whether `text` carries a real conflict: an opening, a divider and a
/// closing marker in that order, each at a line start; a row of `=` alone is
/// prose, and a licence's heading rule is one.
pub fn has_conflict(text: &str) -> bool {
    let mut stage = 0;
    for l in text.lines() {
        stage = match stage {
            0 if l.starts_with("<<<<<<< ") => 1,
            1 if l == "=======" => 2,
            2 if l.starts_with(">>>>>>> ") => return true,
            s => s,
        };
    }
    false
}

#[cfg(test)]
#[path = "hygiene_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hygiene_rows_tests.rs"]
mod rows_tests;
