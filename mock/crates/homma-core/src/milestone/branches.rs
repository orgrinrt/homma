//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The branches the trunk does not hold, with what landing each would take.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use super::range::numstat;
use super::{MilestoneError, commit, contains, exists, git, git_status};

/// Branch names that are never a candidate for landing.
const NEVER: [&str; 2] = ["badges", "HEAD"];

/// What a reader would do with a branch, read off the numbers beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Call {
    /// It merges into the trunk without a conflict.
    Land,
    /// It conflicts with the trunk, so the trunk is merged into it first.
    MergeTrunk,
    /// Merging it into the trunk gives the trunk's own tree.
    Empty,
    /// It shares no history with the trunk.
    Unrelated,
    /// Git could not compare it with the trunk; the reason is beside it.
    Unreadable,
}

impl Call {
    pub fn word(self) -> &'static str {
        match self {
            Call::Land => "land",
            Call::MergeTrunk => "merge-trunk",
            Call::Empty => "empty",
            Call::Unrelated => "unrelated",
            Call::Unreadable => "unreadable",
        }
    }
}

/// One branch the trunk does not contain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Branch {
    pub name:      String,
    pub tip:       String,
    pub commits:   u64,
    pub changed:   u64,
    pub conflicts: Vec<String>,
    pub call:      Call,
    /// Git's words, for the calls that are about git failing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason:    Option<String>,
}

/// What merging a branch into the trunk came to.
enum Merged {
    /// No conflict; the tree the merge made.
    Clean(String),
    /// The files in conflict.
    Conflicted(Vec<String>),
    /// Git refused to say, and why.
    Refused(String),
}

fn merged(root: &Path, trunk: &str, tip: &str) -> Result<Merged, MilestoneError> {
    let out = git_status(root, &[
        "merge-tree",
        "--write-tree",
        "--name-only",
        trunk,
        tip,
    ])?;
    Ok(match out.status {
        Some(0) => Merged::Clean(out.stdout.lines().next().unwrap_or_default().to_string()),
        Some(1) => {
            Merged::Conflicted(
                out.stdout
                    .lines()
                    .skip(1)
                    .take_while(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            )
        },
        _ => Merged::Refused(first_words(&out.stderr, &out.stdout)),
    })
}

/// The first non-empty line git said, from its error stream or else its output.
fn first_words(stderr: &str, stdout: &str) -> String {
    [stderr, stdout]
        .iter()
        .flat_map(|s| s.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git gave no reason")
        .to_string()
}

/// Read one branch against the trunk. A git failure here is this branch's
/// call and never the report's.
fn read(root: &Path, trunk: &str, name: String, tip: String) -> Branch {
    let mut branch = Branch {
        name,
        tip: tip.clone(),
        commits: 0,
        changed: 0,
        conflicts: Vec::new(),
        call: Call::Land,
        reason: None,
    };
    let outcome = (|| -> Result<(), MilestoneError> {
        let range = format!("{trunk}..{tip}");
        branch.commits = git(root, &["rev-list", "--count", &range])?
            .trim()
            .parse()
            .unwrap_or(0);
        if !git_status(root, &["merge-base", trunk, &tip])?.ok() {
            branch.call = Call::Unrelated;
            return Ok(());
        }
        let sym = format!("{trunk}...{tip}");
        branch.changed = numstat(root, &[&sym])?.iter().map(|(_, a, d)| a + d).sum();
        match merged(root, trunk, &tip)? {
            Merged::Clean(tree) => {
                let trunk_tree = git(root, &["rev-parse", &format!("{trunk}^{{tree}}")])?;
                branch.call = if tree == trunk_tree.trim() { Call::Empty } else { Call::Land };
            },
            Merged::Conflicted(files) => {
                branch.conflicts = files;
                branch.call = Call::MergeTrunk;
            },
            Merged::Refused(why) => {
                branch.call = Call::Unreadable;
                branch.reason = Some(why);
            },
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        branch.call = Call::Unreadable;
        branch.reason = Some(e.to_string());
    }
    branch
}

/// Every branch, from `origin` and the local heads, that `trunk` does not
/// contain, except the trunk, the release line, `badges` and `HEAD`.
pub fn branches(root: &Path, trunk: &str, release: &str) -> Result<Vec<Branch>, MilestoneError> {
    let remote = format!("origin/{trunk}");
    let trunk_ref = if exists(root, &remote) { remote } else { trunk.to_string() };
    commit(root, &trunk_ref)?;
    let listing = git(root, &[
        "for-each-ref",
        "--format=%(refname)",
        "refs/heads",
        "refs/remotes/origin",
    ])?;
    // a name held both locally and on origin is read at origin's tip, since
    // the local one may be stale
    let mut refs: BTreeMap<String, String> = BTreeMap::new();
    for r in listing.lines() {
        if let Some(name) = r.strip_prefix("refs/remotes/origin/") {
            refs.insert(name.to_string(), r.to_string());
        } else if let Some(name) = r.strip_prefix("refs/heads/") {
            refs.entry(name.to_string())
                .or_insert_with(|| r.to_string());
        }
    }
    let mut out = Vec::new();
    for (name, full) in refs {
        if name == trunk || name == release || NEVER.contains(&name.as_str()) {
            continue;
        }
        let tip = commit(root, &full)?;
        if contains(root, &trunk_ref, &tip)? {
            continue;
        }
        out.push(read(root, &trunk_ref, name, tip));
    }
    Ok(out)
}
