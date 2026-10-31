//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone: where a landing round began, and the reports over the range
//! since. `DEEPDIVE_milestone.md` in this crate's design is the whole of it;
//! each module here is one section.

use std::fmt;
use std::path::Path;

use crate::release::sh;

pub mod branches;
pub mod range;
pub mod record;
pub mod steps;

#[cfg(test)]
mod tests;

/// What a repository with no mark and no milestone tag is told.
pub const NO_MILESTONE: &str = "no milestone is marked and no `milestone/*` tag is on the trunk; \
                                `homma milestone mark <repo> <sha>` records one";

/// A milestone call that was refused, or a git call under it that failed.
#[derive(Debug)]
pub enum MilestoneError {
    /// `git` exited non-zero; the command line and its stderr.
    Git {
        command: String,
        stderr:  String,
    },
    /// `git` could not be started.
    Spawn(sh::Spawn),
    /// A revision that does not name a commit.
    NotACommit(String),
    /// A range whose base the head does not contain.
    NotAncestor {
        base: String,
        head: String,
    },
    /// A mark of a commit the trunk does not contain.
    NotOnTrunk {
        sha:   String,
        trunk: String,
    },
    /// A mark of a commit already marked.
    AlreadyMarked(String),
    /// A freeze while the last milestone has no tip; its number.
    StillOpen(usize),
    /// A pin with no milestone open.
    NotOpen,
    /// A pin on an open milestone with no freeze recorded; its number.
    NoFreeze(usize),
    /// A pin on a milestone already pinned; its number.
    AlreadyPinned(usize),
    /// A mark of a milestone not yet pinned; its number.
    NotPinned(usize),
    /// A revision that does not contain the one it must follow.
    NotDescendant {
        sha:  String,
        of:   String,
        what: &'static str,
    },
    /// A milestone number no row has.
    NoSuchMilestone(usize),
    /// A repository with no record and no milestone tag.
    NoMilestone,
    /// The record file could not be read as the shape it has.
    Record(String),
    /// The record file or its directory could not be written.
    Io(std::io::Error),
}

impl fmt::Display for MilestoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MilestoneError::Git {
                command,
                stderr,
            } => write!(f, "`{command}` failed: {}", stderr.trim()),
            MilestoneError::Spawn(s) => write!(f, "{s}"),
            MilestoneError::NotACommit(rev) => write!(f, "`{rev}` is not a commit"),
            MilestoneError::NotAncestor {
                base,
                head,
            } => write!(f, "`{base}` is not an ancestor of `{head}`"),
            MilestoneError::NotOnTrunk {
                sha,
                trunk,
            } => write!(f, "`{sha}` is not on `{trunk}`"),
            MilestoneError::AlreadyMarked(sha) => write!(f, "`{sha}` is already marked"),
            MilestoneError::StillOpen(n) => {
                write!(
                    f,
                    "milestone {n} is still open; `homma milestone mark` closes it"
                )
            },
            MilestoneError::NotOpen => {
                f.write_str("no milestone is open; `homma milestone freeze` opens one")
            },
            MilestoneError::NoFreeze(n) => write!(f, "milestone {n} has no freeze recorded"),
            MilestoneError::AlreadyPinned(n) => write!(f, "milestone {n} is already pinned"),
            MilestoneError::NotPinned(n) => {
                write!(
                    f,
                    "milestone {n} is not pinned; `homma milestone pin` sets its head"
                )
            },
            MilestoneError::NotDescendant {
                sha,
                of,
                what,
            } => write!(f, "`{sha}` does not contain the {what} `{of}`"),
            MilestoneError::NoSuchMilestone(n) => write!(f, "no milestone {n} is recorded"),
            MilestoneError::NoMilestone => f.write_str(NO_MILESTONE),
            MilestoneError::Record(why) => write!(f, "the milestone record is unreadable: {why}"),
            MilestoneError::Io(e) => write!(f, "the milestone record could not be written: {e}"),
        }
    }
}

impl std::error::Error for MilestoneError {}

impl From<sh::Spawn> for MilestoneError {
    fn from(s: sh::Spawn) -> Self {
        MilestoneError::Spawn(s)
    }
}

/// Run `git` in `cwd`; an exit other than zero is the error.
pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String, MilestoneError> {
    let out = git_status(cwd, args)?;
    if out.ok() {
        Ok(out.stdout)
    } else {
        Err(MilestoneError::Git {
            command: out.command_line(),
            stderr:  out.stderr,
        })
    }
}

/// Run `git` in `cwd` and hand back the output whatever its exit, for the
/// calls whose exit is the answer.
pub(crate) fn git_status(cwd: &Path, args: &[&str]) -> Result<sh::Output, MilestoneError> {
    Ok(sh::run(cwd, "git", args)?)
}

/// The full sha of the commit `rev` names.
pub fn commit(cwd: &Path, rev: &str) -> Result<String, MilestoneError> {
    let spec = format!("{rev}^{{commit}}");
    let out = git_status(cwd, &["rev-parse", "--verify", "--quiet", &spec])?;
    if out.ok() {
        Ok(out.stdout.trim().to_string())
    } else {
        Err(MilestoneError::NotACommit(rev.to_string()))
    }
}

/// Whether `ancestor` is reachable from `descendant`, a commit being its own.
pub fn contains(cwd: &Path, descendant: &str, ancestor: &str) -> Result<bool, MilestoneError> {
    let out = git_status(cwd, &["merge-base", "--is-ancestor", ancestor, descendant])?;
    match out.status {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            Err(MilestoneError::Git {
                command: out.command_line(),
                stderr:  out.stderr,
            })
        },
    }
}

/// Whether `rev` names a commit here.
pub(crate) fn exists(cwd: &Path, rev: &str) -> bool {
    commit(cwd, rev).is_ok()
}

/// The trunk as this clone holds it: the local branch, or `origin`'s where
/// the clone has only that.
pub(crate) fn trunk_here(cwd: &Path, trunk: &str) -> String {
    if exists(cwd, trunk) { trunk.to_string() } else { format!("origin/{trunk}") }
}
