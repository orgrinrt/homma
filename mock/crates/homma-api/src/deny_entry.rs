//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a manifest's `deny` names, and how one entry resolves to a place.
//!
//! Beside [`Denied`](crate::Denied) rather than inside it, because the two
//! answer different questions. This is the wire form an operator writes and the
//! rules for reading it; that is the list a write is checked against and the
//! comparisons that check it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::AbsPath;

/// One entry of a manifest's `deny`: a place homma may not write, and why.
///
/// Written either as a bare path, or as a table carrying a reason:
///
/// ```toml
/// deny = [
///     "~/work/someone-elses",
///     { path = "scratch", why = "regenerated, and not worth a merge conflict" },
/// ]
/// ```
///
/// The reason is worth having because every refusal in this module tells the
/// operator which place stopped it and on what grounds, and "the workspace
/// manifest denies writes there" is the least useful sentence that could be
/// said. It is optional because a path alone is often self-explanatory to the
/// person who wrote it down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "DenyRepr", into = "DenyRepr")]
pub struct DenyEntry {
    /// Absolute, or relative to the manifest, or `~/`-prefixed.
    pub path: PathBuf,
    /// Shown in the refusal. Defaulted when absent.
    pub why:  Option<String>,
}

impl DenyEntry {
    /// The absolute place this names, or nothing when it cannot be resolved.
    ///
    /// `~/` needs a home and there may not be one, and an entry that cannot be
    /// resolved denotes no place. Returning nothing is right rather than
    /// falling back to another anchor: a denial resolved against the wrong base
    /// refuses a directory the operator never named, which is worse than the
    /// entry having no effect and is much harder to diagnose.
    pub fn resolve(&self, base: &AbsPath, home: Option<&AbsPath>) -> Option<AbsPath> {
        let raw = self.path.as_path();
        let mut components = raw.components();
        if raw.starts_with("~") {
            let home = home?;
            components.next();
            let rest: PathBuf = components.collect();
            return Some(if rest.as_os_str().is_empty() { home.clone() } else { home.join(&rest) });
        }
        Some(AbsPath::resolve(base, raw))
    }
}

/// Both spellings a `deny` entry takes on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum DenyRepr {
    Bare(PathBuf),
    Table {
        path: PathBuf,
        #[serde(default)]
        why:  Option<String>,
    },
}

impl From<DenyRepr> for DenyEntry {
    fn from(r: DenyRepr) -> Self {
        match r {
            DenyRepr::Bare(path) => {
                Self {
                    path,
                    why: None,
                }
            },
            DenyRepr::Table {
                path,
                why,
            } => {
                Self {
                    path,
                    why,
                }
            },
        }
    }
}

impl From<DenyEntry> for DenyRepr {
    fn from(e: DenyEntry) -> Self {
        // Round-trips to the shorter spelling when there is nothing to carry,
        // so a manifest homma writes back reads the way one written by hand does.
        match e.why {
            None => DenyRepr::Bare(e.path),
            Some(why) => {
                DenyRepr::Table {
                    path: e.path,
                    why:  Some(why),
                }
            },
        }
    }
}
