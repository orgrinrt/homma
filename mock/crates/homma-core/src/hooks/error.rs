//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//! Why the git hooks were not registered, each said with what to do.

use std::fmt;
use std::path::PathBuf;

use crate::release::git::GitError;

/// Why nothing was registered.
#[derive(Debug)]
pub enum HookError {
    /// git-shook does not hold the repository's `core.hooksPath`, and homma
    /// never takes that key.
    NotShook(PathBuf),
    /// The workspace's git hooks manifest cannot be read as a toml.
    Manifest {
        path:   PathBuf,
        reason: String,
    },
    /// The workspace's git hooks manifest is named in a way the copy cannot
    /// carry: `homma`, which is the gate's, or not a plain file name.
    Name {
        path: PathBuf,
        name: String,
    },
    /// Registering through git-shook did not go: it is not installed, it
    /// refused, or it registered less than the manifest declares. The text
    /// says which.
    Shook(String),
    Git(GitError),
    Io(std::io::Error),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::NotShook(root) => {
                write!(
                    f,
                    "git-shook does not hold the hooks path of {}, and homma never takes it: run \
                     `git shook install` there, then this again",
                    root.display()
                )
            },
            HookError::Manifest {
                path,
                reason,
            } => write!(f, "{} cannot be read: {reason}", path.display()),
            HookError::Name {
                path,
                name,
            } => {
                write!(
                    f,
                    "{} is named \"{name}\", which the copy cannot carry: give it a name of \
                     letters, digits, dot, dash and underscore other than \"homma\", which is the \
                     release gate's",
                    path.display()
                )
            },
            HookError::Shook(why) => write!(f, "{why}"),
            HookError::Git(e) => write!(f, "{e}"),
            HookError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HookError {}

impl From<GitError> for HookError {
    fn from(e: GitError) -> Self {
        HookError::Git(e)
    }
}

impl From<std::io::Error> for HookError {
    fn from(e: std::io::Error) -> Self {
        HookError::Io(e)
    }
}
