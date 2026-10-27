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
    /// It holds commits and differs from the trunk by no line.
    Empty,
}

impl Call {
    pub fn word(self) -> &'static str {
        match self {
            Call::Land => "land",
            Call::MergeTrunk => "merge-trunk",
            Call::Empty => "empty",
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
}

/// The files `git merge-tree --write-tree --name-only` reports in conflict.
fn conflicts(root: &Path, trunk: &str, tip: &str) -> Result<Vec<String>, MilestoneError> {
    let out = git_status(root, &[
        "merge-tree",
        "--write-tree",
        "--name-only",
        trunk,
        tip,
    ])?;
    match out.status {
        Some(0) => Ok(Vec::new()),
        Some(1) => {
            Ok(out
                .stdout
                .lines()
                .skip(1)
                .take_while(|l| !l.is_empty())
                .map(str::to_string)
                .collect())
        },
        _ => {
            Err(MilestoneError::Git {
                command: out.command_line(),
                stderr:  out.stderr,
            })
        },
    }
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
        let range = format!("{trunk_ref}..{tip}");
        let commits = git(root, &["rev-list", "--count", &range])?
            .trim()
            .parse()
            .unwrap_or(0);
        let sym = format!("{trunk_ref}...{tip}");
        let changed = numstat(root, &[&sym])?.iter().map(|(_, a, d)| a + d).sum();
        let conflicts = conflicts(root, &trunk_ref, &tip)?;
        let call = if !conflicts.is_empty() {
            Call::MergeTrunk
        } else if changed == 0 {
            Call::Empty
        } else {
            Call::Land
        };
        out.push(Branch {
            name,
            tip,
            commits,
            changed,
            conflicts,
            call,
        });
    }
    Ok(out)
}
