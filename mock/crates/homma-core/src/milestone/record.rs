//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The record: `.homma/milestones.toml`, one table per mark, appended in
//! order, and the milestone tags that answer a repository without one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{MilestoneError, commit, contains, exists, git, trunk_here};

/// Where the record sits, relative to the repository's root.
pub const RECORD_PATH: &str = ".homma/milestones.toml";

/// The prefix of a milestone tag.
pub const TAG_PREFIX: &str = "milestone/";

/// One mark: the commit and its own committer date.
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

#[derive(Deserialize, Default)]
struct File {
    #[serde(default)]
    milestone: Vec<Mark>,
}

/// The marks in the record, oldest first; none where there is no file.
pub fn marks(root: &Path) -> Result<Vec<Mark>, MilestoneError> {
    let path = root.join(RECORD_PATH);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(MilestoneError::Record(e.to_string())),
    };
    let file: File = toml::from_str(&text).map_err(|e| MilestoneError::Record(e.to_string()))?;
    Ok(file.milestone)
}

/// The current milestone: the last mark, or the newest `milestone/*` tag the
/// trunk contains where the record has none.
pub fn base(root: &Path, trunk: &str) -> Result<(String, Source), MilestoneError> {
    if let Some(last) = marks(root)?.pop() {
        return Ok((last.sha, Source::Record));
    }
    let listing = git(root, &[
        "tag",
        "--list",
        "milestone/*",
        "--merged",
        &trunk_here(root, trunk),
        "--sort=-version:refname",
    ])?;
    match listing.lines().next() {
        Some(tag) => Ok((commit(root, tag)?, Source::Tag)),
        None => Err(MilestoneError::NoMilestone),
    }
}

/// Whether the trunk, here or on origin, contains `sha`.
fn on_trunk(root: &Path, trunk: &str, sha: &str) -> Result<bool, MilestoneError> {
    if exists(root, trunk) && contains(root, trunk, sha)? {
        return Ok(true);
    }
    let remote = format!("origin/{trunk}");
    Ok(exists(root, &remote) && contains(root, &remote, sha)?)
}

/// Mark `rev`: append it to the record and, with `tag`, make the annotated tag
/// `milestone/<n>` here. Commits nothing and pushes nothing.
pub fn mark(root: &Path, trunk: &str, rev: &str, tag: bool) -> Result<Mark, MilestoneError> {
    let sha = commit(root, rev)?;
    if !on_trunk(root, trunk, &sha)? {
        return Err(MilestoneError::NotOnTrunk {
            sha:   rev.to_string(),
            trunk: trunk.to_string(),
        });
    }
    let existing = marks(root)?;
    if existing.iter().any(|m| m.sha == sha) {
        return Err(MilestoneError::AlreadyMarked(rev.to_string()));
    }
    let at = git(root, &["show", "-s", "--format=%cs", &sha])?
        .trim()
        .to_string();
    let new = Mark {
        sha,
        at,
    };
    // the tag first: a refused tag then leaves no mark behind, and a rerun is
    // not met by "already marked"
    let tagged = if tag { Some(make_tag(root, existing.len() + 1, &new.sha)?) } else { None };
    let path = root.join(RECORD_PATH);
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&format!(
        "[[milestone]]\nsha = \"{}\"\nat = \"{}\"\n",
        new.sha, new.at
    ));
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, text));
    if let Err(e) = written {
        if let Some(name) = tagged {
            let _ = git(root, &["tag", "-d", &name]);
        }
        return Err(MilestoneError::Io(e));
    }
    Ok(new)
}

/// Make the annotated tag `milestone/<n>` at `sha`, `n` being the first
/// number from `from` up whose tag is free; its name.
fn make_tag(root: &Path, from: usize, sha: &str) -> Result<String, MilestoneError> {
    let mut n = from;
    let name = loop {
        let name = format!("{TAG_PREFIX}{n}");
        if !exists(root, &name) {
            break name;
        }
        n += 1;
    };
    git(root, &[
        "tag",
        "-a",
        &name,
        sha,
        "-m",
        &format!("milestone {n}"),
    ])?;
    Ok(name)
}
