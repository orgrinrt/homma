//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The record: `.homma/milestones.toml`, one table per milestone, in order,
//! and the milestone tags that answer a repository without one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{MilestoneError, commit, contains, exists, git, trunk_here};

/// Where the record sits, relative to the repository's root.
pub const RECORD_PATH: &str = ".homma/milestones.toml";

/// The prefix of a milestone tag.
pub const TAG_PREFIX: &str = "milestone/";

/// One milestone as recorded. Every field but the revisions it was made of is
/// optional because a row is written in steps, and a row from before the steps
/// holds `sha` and `at` only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Its number; a row without one is numbered by its place in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n:         Option<usize>,
    /// The previous milestone's tip, absent where none is recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base:      Option<String>,
    /// The trunk's tip when the freeze began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze:    Option<String>,
    /// The committer date of `freeze`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_at: Option<String>,
    /// The revision the review pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head:      Option<String>,
    /// The trunk's tip at close, the next milestone's base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip:       Option<String>,
    /// A row from before the steps: its `tip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha:       Option<String>,
    /// The committer date of the tip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at:        Option<String>,
}

impl Row {
    /// Where the milestone closed: `tip`, or `sha` on a row of the old shape.
    pub fn tip(&self) -> Option<&str> {
        self.tip.as_deref().or(self.sha.as_deref())
    }
}

/// A close made: the tip and the date of its commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub sha: String,
    pub at:  String,
}

/// Where `base` found its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Record,
    Tag,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    milestone: Vec<Row>,
}

/// The number of the row at `index`: its own, or its place in the file.
pub fn number(rows: &[Row], index: usize) -> usize {
    rows[index].n.unwrap_or(index + 1)
}

/// The rows in the working tree's record, oldest first; none where there is
/// no file. The steps write to this one.
pub fn rows(root: &Path) -> Result<Vec<Row>, MilestoneError> {
    let path = root.join(RECORD_PATH);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(MilestoneError::Record(e.to_string())),
    };
    parse(&text)
}

fn parse(text: &str) -> Result<Vec<Row>, MilestoneError> {
    let file: File = toml::from_str(text).map_err(|e| MilestoneError::Record(e.to_string()))?;
    Ok(file.milestone)
}

/// Write `rows` as the record, creating its directory. A row is written in the
/// shape it was read in: the fields it has and no others.
pub fn write(root: &Path, rows: Vec<Row>) -> Result<(), MilestoneError> {
    let text = toml::to_string(&File {
        milestone: rows,
    })
    .map_err(|e| MilestoneError::Record(e.to_string()))?;
    let path = root.join(RECORD_PATH);
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, text))
        .map_err(MilestoneError::Io)
}

/// The rows in the record as the trunk commits it, oldest first; none where
/// the trunk has no file. A clone checked out on a branch answers as the trunk.
pub fn trunk_rows(root: &Path, trunk_ref: &str) -> Result<Vec<Row>, MilestoneError> {
    let spec = format!("{trunk_ref}:{RECORD_PATH}");
    let listing = git(root, &[
        "ls-tree",
        "--name-only",
        trunk_ref,
        "--",
        RECORD_PATH,
    ])?;
    if listing.trim().is_empty() {
        return Ok(Vec::new());
    }
    parse(&git(root, &["show", &spec])?)
}

/// The newest `milestone/*` tag the trunk contains, as the commit it names.
pub(super) fn newest_tag(root: &Path, trunk_ref: &str) -> Result<Option<String>, MilestoneError> {
    let listing = git(root, &[
        "tag",
        "--list",
        "milestone/*",
        "--merged",
        trunk_ref,
        "--sort=-version:refname",
    ])?;
    listing
        .lines()
        .next()
        .map(|tag| commit(root, tag))
        .transpose()
}

/// Where the current milestone began: the last tip in the trunk's committed
/// record, or the newest `milestone/*` tag the trunk contains where the record
/// has none. While a milestone is open this is the sha it began from.
pub fn base(root: &Path, trunk: &str) -> Result<(String, Source), MilestoneError> {
    let trunk_ref = trunk_here(root, trunk);
    let tip = trunk_rows(root, &trunk_ref)?
        .iter()
        .rev()
        .find_map(|r| r.tip().map(str::to_string));
    if let Some(sha) = tip {
        return Ok((sha, Source::Record));
    }
    match newest_tag(root, &trunk_ref)? {
        Some(sha) => Ok((sha, Source::Tag)),
        None => Err(MilestoneError::NoMilestone),
    }
}

/// Whether the trunk, here or on origin, contains `sha`.
pub(super) fn on_trunk(root: &Path, trunk: &str, sha: &str) -> Result<bool, MilestoneError> {
    if exists(root, trunk) && contains(root, trunk, sha)? {
        return Ok(true);
    }
    let remote = format!("origin/{trunk}");
    Ok(exists(root, &remote) && contains(root, &remote, sha)?)
}
