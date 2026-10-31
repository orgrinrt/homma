//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which repositories a clone of the workspace works on.
//!
//! `repos` under `[instance]` in `homma.local.toml` names them. This module is
//! the one reader of that key: [`Config::from_path`](crate::Config::from_path)
//! asks it before it opens a single directory, and `homma what` asks it for the
//! repositories it runs mockspace in. A command that needs the set of repos
//! takes it from the config built with the scope, and never reads the local
//! file for the list itself.
//!
//! A repository outside the scope is not opened, fetched or run in, so the work
//! is skipped rather than done and then hidden from the output.

use std::collections::BTreeSet;
use std::path::Path;

use crate::local::{Local, LocalError};

/// The repositories in scope.
///
/// Absent scope means every repository: a clone with no `homma.local.toml`, a
/// file naming none, or a file that cannot be read all give [`Scope::everything`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// `None` is everything, which no named set can express.
    names: Option<BTreeSet<String>>,
}

impl Scope {
    /// No narrowing: every repository is in scope.
    pub const fn everything() -> Self {
        Self {
            names: None,
        }
    }

    /// Exactly the named repositories. An empty list narrows nothing, the way
    /// `repos = []` in the local file does.
    pub fn named<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let set: BTreeSet<String> = names.into_iter().map(Into::into).collect();
        if set.is_empty() {
            Self::everything()
        } else {
            Self {
                names: Some(set),
            }
        }
    }

    /// Whether `repo` is a repository the work touches.
    pub fn admits(&self, repo: &str) -> bool {
        self.names.as_ref().is_none_or(|n| n.contains(repo))
    }

    /// The named repositories, `None` where the scope narrows nothing.
    pub const fn names(&self) -> Option<&BTreeSet<String>> {
        self.names.as_ref()
    }

    /// The names in scope that `present` does not hold: repositories the config
    /// names and no clone provides. Not an error: a container that has not
    /// attached one yet is the ordinary case.
    pub fn unmet<'a, I>(&self, present: I) -> Vec<&str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let present: BTreeSet<&str> = present.into_iter().collect();
        self.names
            .iter()
            .flatten()
            .map(String::as_str)
            .filter(|n| !present.contains(n))
            .collect()
    }

    /// The scope of the local file in `dir`.
    ///
    /// A file that is not there gives everything and no error. One that is
    /// there and cannot be read also gives everything, with the error, so a
    /// caller can say its scope was not applied rather than refuse: a broken
    /// local file must not lock out the command that repairs it.
    pub fn load(dir: &Path) -> (Self, Option<LocalError>) {
        match Local::load(dir) {
            Ok(local) => {
                (
                    local.map_or_else(Self::everything, |l| Self::named(l.instance.repos)),
                    None,
                )
            },
            Err(e) => (Self::everything(), Some(e)),
        }
    }
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
