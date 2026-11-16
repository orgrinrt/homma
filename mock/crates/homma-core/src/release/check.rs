//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The invariants a release rests on, each reported by the id the check
//! catalogue gave it. The blocking set is what `DEEPDIVE_release.md` names;
//! the rest is reported at warn and holds nothing up.

use std::collections::BTreeMap;
use std::path::Path;

use homma_api::{CheckSeverity, Finding, Markers, RepoKind, Target, Version};

pub use super::packages::{Packages, Published, fetch_published, packages, tag_name, tag_version};
use super::registry::Registry;
use super::workspace::{self, Layout};
use super::{git, kind, version};

/// What the check needs to know about the repo, beyond the tree itself.
pub struct Inputs<'a> {
    pub root:      &'a Path,
    /// The repository as the workspace manifest names it, which names a
    /// virtual workspace's primary.
    pub repo:      &'a str,
    pub remote:    &'a str,
    pub trunk:     &'a str,
    pub release:   &'a str,
    pub target:    Option<Target>,
    pub published: &'a Published,
    pub markers:   &'a Markers,
}

/// Every finding, blocking ones first.
pub fn check(inputs: &Inputs<'_>) -> Result<Vec<Finding>, git::GitError> {
    let root = inputs.root;
    let mut out = Vec::new();
    let mut push = |id: &str, sev: CheckSeverity, msg: String| out.push(Finding::new(id, sev, msg));

    // the tree
    let modified = git::modified(root)?;
    if !modified.is_empty() {
        push(
            "tree.clean",
            CheckSeverity::Error,
            format!("modified: {}", modified.join(", ")),
        );
    }
    let untracked = git::untracked(root)?;
    if !untracked.is_empty() {
        push(
            "tree.untracked",
            CheckSeverity::Error,
            format!("untracked: {}", untracked.join(", ")),
        );
    }
    let branch = git::current_branch(root)?;
    match &branch {
        None => {
            push(
                "tree.attached",
                CheckSeverity::Error,
                "HEAD is detached".into(),
            )
        },
        Some(b) => {
            if !git::is_pushed(root, inputs.remote, b)? {
                push(
                    "tree.pushed",
                    CheckSeverity::Error,
                    format!("`{b}` has commits `{}` lacks", inputs.remote),
                );
            }
        },
    }
    let tracked = git::tracked_at(root, "HEAD")?;
    let workflows: Vec<&String> = tracked
        .iter()
        .filter(|p| p.starts_with(".github/workflows/"))
        .collect();
    if !workflows.is_empty() {
        push(
            "hist.workflow.tree",
            CheckSeverity::Error,
            format!("{} workflow file(s) in the tree", workflows.len()),
        );
    }

    // the tags
    let tags = git::tags(root)?;
    let mut versioned: Vec<(String, Version)> = tags
        .iter()
        .filter_map(|t| tag_version(t).map(|v| (t.clone(), v)))
        .collect();
    versioned.sort_by(|a, b| a.1.cmp(&b.1));
    let bare = versioned
        .iter()
        .filter(|(t, _)| !t.starts_with('v'))
        .count();
    if bare != 0 && bare != versioned.len() {
        push(
            "tag.prefix",
            CheckSeverity::Warn,
            format!("{bare} of {} tags lack the `v`", versioned.len()),
        );
    }
    let mut by_target: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (t, _) in &versioned {
        if !git::tag_is_annotated(root, t)? {
            push(
                "tag.annotated",
                CheckSeverity::Warn,
                format!("`{t}` is lightweight"),
            );
        }
        let target = git::tag_target(root, t)?;
        if !git::is_ancestor(root, &target, inputs.release)? {
            push(
                "tag.reachable",
                CheckSeverity::Error,
                format!("`{t}` is not on `{}`", inputs.release),
            );
        }
        by_target.entry(target).or_default().push(t);
    }
    for (target, names) in &by_target {
        if names.len() > 1 {
            push(
                "tag.dupes",
                CheckSeverity::Warn,
                format!(
                    "{} share {}",
                    names.join(", "),
                    &target[.. 7.min(target.len())]
                ),
            );
        }
    }
    let remote_tags = git::remote_tags(root, inputs.remote)?;
    for (t, _) in &versioned {
        match remote_tags.iter().find(|(n, _)| n == t) {
            None => {
                push(
                    "tag.pushed",
                    CheckSeverity::Error,
                    format!("`{t}` is not on `{}`", inputs.remote),
                )
            },
            Some((_, remote_sha)) => {
                if *remote_sha != git::tag_target(root, t)? {
                    push(
                        "tag.sha.agrees",
                        CheckSeverity::Fatal,
                        format!("`{t}` points elsewhere on `{}`", inputs.remote),
                    );
                }
            },
        }
    }
    for (name, _) in &remote_tags {
        if tag_version(name).is_some() && !tags.contains(name) {
            push(
                "tag.local",
                CheckSeverity::Warn,
                format!("`{name}` is on `{}` and not here", inputs.remote),
            );
        }
    }

    // main holds nothing unreleased; where the trunk is the release line the
    // unreleased commits are what the release is for, so nothing to hold
    let release_sha = git::sha(root, inputs.release).ok();
    if let (Some(release_sha), Some((newest, _)), true) = (
        &release_sha,
        versioned.last(),
        inputs.trunk != inputs.release,
    ) {
        let newest_target = git::tag_target(root, newest)?;
        if newest_target != *release_sha {
            let between = git::subjects(root, newest, inputs.release)?;
            let hotpatch = between.iter().all(|s| names_hotpatch(&s.subject));
            if !hotpatch {
                push(
                    "main.unreleased",
                    CheckSeverity::Error,
                    format!(
                        "{} commit(s) on `{}` past `{newest}`",
                        between.len(),
                        inputs.release
                    ),
                );
            }
        }
    }

    // the manifest against the tags, and the working version; a virtual
    // workspace's primary reads as the repo's version, through the root
    // manifest from before the repo was a workspace and its own since
    let repo_kind = kind::detect(root, inputs.markers).ok();
    let layout = match repo_kind.map(|k| workspace::layout(root, k, inputs.repo)) {
        Some(Ok(l)) => Some(l),
        Some(Err(e)) => {
            push("man.primary", CheckSeverity::Error, e.to_string());
            None
        },
        None => None,
    };
    let primary = match &layout {
        Some(Layout::Set {
            primary,
            ..
        }) => Some(primary),
        _ => None,
    };
    let riders: &[workspace::Member] = match &layout {
        Some(Layout::Set {
            riders,
            ..
        }) => riders,
        _ => &[],
    };
    let working = match (repo_kind, &layout) {
        (Some(k), Some(l)) => super::plan::working_version(root, k, l).ok(),
        _ => None,
    };
    let version_at = |rev: &str, k: RepoKind| -> Result<Option<Version>, git::GitError> {
        if let Some(v) = manifest_version_at(root, rev, k)? {
            return Ok(Some(v));
        }
        match primary {
            Some(p) => workspace::version_at(root, rev, &p.manifest),
            None => Ok(None),
        }
    };
    // every version a bump on the release line moves to is tagged: walk
    // `main` by first parent over the commits that touch the manifest, and
    // where the version differs from the first parent's, a tag naming that
    // version sits on the bump or on a commit after it: a version bumped
    // before the repo tagged anything was tagged on a later commit of it, and
    // that the tag's commit carries the version is `man.version.matches`
    let mut tag_targets: Vec<(Version, String)> = Vec::new();
    for (t, v) in &versioned {
        tag_targets.push((v.clone(), git::tag_target(root, t)?));
    }
    let tagged_since = |sha: &str, v: &Version| -> Result<bool, git::GitError> {
        for (tv, target) in &tag_targets {
            if tv == v
                && git::is_ancestor(root, sha, target)?
                && git::is_ancestor(root, target, inputs.release)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if let (Some(k), Some(_)) = (repo_kind, &release_sha) {
        let file = if k.has_crate() { "Cargo.toml" } else { "deno.json" };
        let mut files = vec![file.to_string()];
        files.extend(primary.map(|p| p.manifest.clone()));
        // a commit both walks find is one finding
        let mut seen: Vec<String> = Vec::new();
        for file in &files {
            for sha in git::first_parent_touching(root, inputs.release, file)? {
                if seen.contains(&sha) {
                    continue;
                }
                seen.push(sha.clone());
                let here = version_at(&sha, k)?;
                let parent = version_at(&format!("{sha}^"), k)?;
                if let (Some(here), Some(parent)) = (here, parent) {
                    if here != parent && !tagged_since(&sha, &here)? {
                        push(
                            "tag.bump.tagged",
                            CheckSeverity::Error,
                            format!("{} bumps to {here} and no tag names it", &sha[.. 7]),
                        );
                    }
                }
            }
        }
    }
    if let Some(k) = repo_kind {
        for (t, v) in &versioned {
            if let Some(mv) = version_at(t, k)? {
                if mv != *v {
                    push(
                        "man.version.matches",
                        CheckSeverity::Fatal,
                        format!("`{t}` carries manifest version {mv}"),
                    );
                }
            }
        }
    }
    for f in super::riders::findings(
        root,
        riders,
        &versioned,
        release_sha.as_ref().map(|_| inputs.release),
        inputs.published,
    )? {
        push(&f.id, f.severity, f.message);
    }
    let highest_published = match primary {
        Some(p) => {
            inputs
                .published
                .get(Registry::CratesIo, &p.name)
                .and_then(|v| v.iter().max().cloned())
        },
        None => inputs.published.versions.values().flatten().max().cloned(),
    };
    // the working version sits at the published one, since the run bumps it
    // after this check, or already one step above it; below is always wrong,
    // and above by more than the target's step is wrong when a target is given
    if let Some(w) = &working {
        if let Some(hp) = &highest_published {
            if w < hp {
                push(
                    "man.current.forward",
                    CheckSeverity::Error,
                    format!("working version {w} is below the published {hp}"),
                );
            } else if let Some(target) = &inputs.target {
                let next = target.next_from(hp);
                if w != hp && *w != next {
                    push(
                        "man.current.smallest",
                        CheckSeverity::Error,
                        format!("{w} is not what a {target} makes of {hp}, {next}"),
                    );
                }
            }
        }
    }

    // the registries
    for ((reg, name), versions) in &inputs.published.versions {
        for pair in versions.windows(2) {
            if pair[1] < pair[0] {
                push(
                    "order.ascends",
                    CheckSeverity::Error,
                    format!("{name} on {reg}: {} after {}", pair[1], pair[0]),
                );
            }
        }
        let mut sorted = versions.clone();
        sorted.sort();
        for pair in sorted.windows(2) {
            if !is_adjacent(&pair[0], &pair[1]) {
                push(
                    "semver.gaps",
                    CheckSeverity::Error,
                    format!("{name} on {reg}: {} skips to {}", pair[0], pair[1]),
                );
            }
        }
        if riders.iter().any(|r| r.name == *name) {
            continue;
        }
        for v in versions {
            if !versioned.iter().any(|(_, tv)| tv == v) {
                push(
                    "reg.orphan",
                    CheckSeverity::Error,
                    format!("{name} {v} is on {reg} with no tag"),
                );
            }
        }
        for (t, tv) in &versioned {
            if !versions.contains(tv) {
                // the newest tag with nothing published for it is a release
                // half done; an older one is history
                let newest = versioned.last().is_some_and(|(n, _)| n == t);
                push(
                    "reg.unpublished",
                    if newest { CheckSeverity::Error } else { CheckSeverity::Warn },
                    format!("`{t}` is not on {reg} as {name}"),
                );
            }
        }
    }
    // a package shipping on both jsr and npm holds the same set on each; the
    // pair is what the repo's own manifests declare, so the two names need
    // share nothing
    let shipped = repo_kind.map(|k| packages(root, k)).unwrap_or_default();
    if let (Some(jsr_name), Some(npm_name)) = (&shipped.jsr, &shipped.npm) {
        let on = |r: Registry, n: &str| {
            let mut v = inputs.published.get(r, n).cloned().unwrap_or_default();
            v.sort();
            v
        };
        if on(Registry::Jsr, jsr_name) != on(Registry::Npm, npm_name) {
            push(
                "both.sameset",
                CheckSeverity::Error,
                format!("{jsr_name} on jsr and {npm_name} on npm hold different version sets"),
            );
        }
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    Ok(out)
}

/// A subject names a hotpatch where `hotpatch` stands as a word of its own,
/// in any case, and neither of the two words before it is a negation; "this
/// is not a hotpatch" and "no hotpatch here" name nothing.
fn names_hotpatch(subject: &str) -> bool {
    let words: Vec<String> = subject
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    words.iter().enumerate().any(|(i, w)| {
        let negated = words[i.saturating_sub(2) .. i]
            .iter()
            .any(|p| p == "not" || p == "no");
        w == "hotpatch" && !negated
    })
}

/// Whether `b` is one step above `a`, as `Version::is_one_step` reads it.
fn is_adjacent(a: &Version, b: &Version) -> bool {
    a.is_one_step(b)
}

fn manifest_version_at(
    root: &Path,
    rev: &str,
    k: RepoKind,
) -> Result<Option<Version>, git::GitError> {
    let file = if k.has_crate() { "Cargo.toml" } else { "deno.json" };
    let Some(text) = git::show(root, rev, file)? else {
        return Ok(None);
    };
    // the scratch manifest is written the way the npmrc is: a path already
    // there is refused, so a planted file is an error and never the answer
    let dir = tempfile_dir();
    if let Err(e) = super::publish::write_private(&dir.join(file), &text) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(git::GitError::Scratch(e));
    }
    let k = if k.has_crate() { RepoKind::Crate } else { RepoKind::Deno };
    let v = version::read(&dir, k).ok();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(v)
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("homma-check-{}-{}", std::process::id(), unique()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Whether any finding blocks.
pub fn blocked(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity.blocks())
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
