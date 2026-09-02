//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The invariants a release rests on, each reported by the id the check
//! catalogue gave it. The blocking set is what `DEEPDIVE_release.md` names;
//! the rest is reported at warn and holds nothing up.

use std::collections::BTreeMap;
use std::path::Path;

use homma_api::{CheckSeverity, Finding, Level, RepoKind, Version};

use super::registry::Registry;
use super::{git, kind, version};

pub use super::packages::{Packages, Published, fetch_published, packages, tag_name, tag_version};

/// What the check needs to know about the repo, beyond the tree itself.
pub struct Inputs<'a> {
    pub root:      &'a Path,
    pub remote:    &'a str,
    pub trunk:     &'a str,
    pub release:   &'a str,
    pub level:     Option<Level>,
    pub published: &'a Published,
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
            let hotpatch = between
                .iter()
                .all(|s| s.subject.to_ascii_lowercase().contains("hotpatch"));
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

    // the manifest against the tags, and the working version
    let repo_kind = kind::detect(root).ok();
    let working = repo_kind.and_then(|k| version::read(root, k).ok());
    // every bump on the release line carries a tag: walk `main` by first
    // parent over the commits that touch the manifest, and where the version
    // differs from the first parent's, a versioned tag points at that commit
    if let (Some(k), Some(_)) = (repo_kind, &release_sha) {
        let file = if k.has_crate() { "Cargo.toml" } else { "deno.json" };
        for sha in git::first_parent_touching(root, inputs.release, file)? {
            let here = manifest_version_at(root, &sha, k)?;
            let parent = manifest_version_at(root, &format!("{sha}^"), k)?;
            if let (Some(here), Some(parent)) = (here, parent) {
                if here != parent && !by_target.contains_key(&sha) {
                    push(
                        "tag.bump.tagged",
                        CheckSeverity::Error,
                        format!(
                            "{} bumps to {here} and no tag points at it",
                            &sha[.. 7]
                        ),
                    );
                }
            }
        }
    }
    if let Some(k) = repo_kind {
        for (t, v) in &versioned {
            let manifest_version = manifest_version_at(root, t, k)?;
            if let Some(mv) = manifest_version {
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
    let highest_published = inputs.published.versions.values().flatten().max().cloned();
    // the working version sits at the published one, since the run bumps it
    // after this check, or already one step above it; below is always wrong,
    // and above by more than the level's step is wrong when a level is given
    if let Some(w) = &working {
        if let Some(hp) = &highest_published {
            if w < hp {
                push(
                    "man.current.forward",
                    CheckSeverity::Error,
                    format!("working version {w} is below the published {hp}"),
                );
            } else if let Some(level) = inputs.level {
                if w != hp && !hp.is_smallest_successor(w, level) {
                    push(
                        "man.current.smallest",
                        CheckSeverity::Error,
                        format!("{w} is not the {level} step above {hp}"),
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
                push(
                    "reg.unpublished",
                    CheckSeverity::Warn,
                    format!("`{t}` is not on {reg} as {name}"),
                );
            }
        }
    }
    let jsr: Vec<_> = inputs
        .published
        .versions
        .iter()
        .filter(|((r, _), _)| *r == Registry::Jsr)
        .map(|(_, v)| v.clone())
        .collect();
    let npm: Vec<_> = inputs
        .published
        .versions
        .iter()
        .filter(|((r, _), _)| *r == Registry::Npm)
        .map(|(_, v)| v.clone())
        .collect();
    if let (Some(j), Some(n)) = (jsr.first(), npm.first()) {
        let mut js = j.clone();
        let mut ns = n.clone();
        js.sort();
        ns.sort();
        if js != ns {
            push(
                "both.sameset",
                CheckSeverity::Warn,
                "jsr and npm hold different version sets".into(),
            );
        }
    }

    out.sort_by(|a, b| b.severity.cmp(&a.severity));
    Ok(out)
}

/// Whether `b` is one legal step above `a` at some level.
fn is_adjacent(a: &Version, b: &Version) -> bool {
    [Level::Patch, Level::Minor, Level::Major]
        .iter()
        .any(|l| &a.bumped(*l) == b)
        || (a.major == 0 && *b == Version::new(1, 0, 0))
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
    let dir = tempfile_dir();
    std::fs::write(dir.join(file), text).ok();
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
