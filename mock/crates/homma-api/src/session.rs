//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Who a session is, as far as the rules it loads are concerned.
//!
//! A rule card binds a session by its **role** and by its **place**, and a
//! session is one of each. The role here is the session's own, a coordinator or
//! a helper, and is not the standing a person has in the organisation, which
//! [`crate::Role`] names.
//!
//! This module reads nothing from the machine. It takes the text of the role
//! file and the value of the cloud variable as arguments, so the rule for what
//! they mean is testable without a home directory or an environment, and the
//! caller owns the two reads.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::rule::RuleMeta;

/// Where the role file is, relative to the home directory.
///
/// The file the setup script and the bootstrap write: one word, the role.
pub const ROLE_FILE: &str = ".config/clause-dev/role";

/// The variable marking a session as one in a cloud container.
pub const CLOUD_VAR: &str = "CLAUSE_CLOUD";

/// What a session does in the workspace's runs of helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionRole {
    /// Runs the helpers: checks them, lands their work, starts the next.
    Coordinator,
    /// Does one job, started by a coordinator.
    Helper,
}

impl SessionRole {
    /// Every role, in the order reports list them.
    pub const ALL: [Self; 2] = [Self::Coordinator, Self::Helper];

    /// The word the role file and a rule's `roles` key spell it with.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Coordinator => "coordinator",
            Self::Helper => "helper",
        }
    }
}

impl fmt::Display for SessionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl FromStr for SessionRole {
    type Err = UnknownWord;

    fn from_str(s: &str) -> Result<Self, UnknownWord> {
        Self::ALL
            .into_iter()
            .find(|r| r.word() == s)
            .ok_or_else(|| UnknownWord::new("a role", s, &Self::ALL.map(Self::word)))
    }
}

/// Where a session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    /// A cloud container, with `CLAUSE_CLOUD` set to `1`.
    Cloud,
    /// Anywhere else.
    Local,
}

impl Place {
    /// Every place, in the order reports list them.
    pub const ALL: [Self; 2] = [Self::Cloud, Self::Local];

    /// The word a rule's `where` key spells it with.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Local => "local",
        }
    }

    /// The place a value of the cloud variable says.
    ///
    /// **Exactly `1` is a cloud container.** The workspace's bootstrap reads it
    /// the same way, and `CLAUSE_CLOUD=0` is written on purpose to say it is not
    /// one, so "set" would call that session a cloud one.
    pub const fn from_cloud_var(value: Option<&str>) -> Self {
        // Bytes rather than a string pattern: `str` equality is not yet const.
        match value {
            Some(v) if v.len() == 1 && v.as_bytes()[0] == b'1' => Self::Cloud,
            _ => Self::Local,
        }
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl FromStr for Place {
    type Err = UnknownWord;

    fn from_str(s: &str) -> Result<Self, UnknownWord> {
        Self::ALL
            .into_iter()
            .find(|p| p.word() == s)
            .ok_or_else(|| UnknownWord::new("a place", s, &Self::ALL.map(Self::word)))
    }
}

/// A word that is not one of the closed set it was read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownWord {
    what:  &'static str,
    got:   String,
    known: String,
}

impl UnknownWord {
    fn new(what: &'static str, got: &str, known: &[&str]) -> Self {
        Self {
            what,
            got: got.to_string(),
            known: known.join("`, `"),
        }
    }

    /// The word that was refused.
    pub const fn got(&self) -> &str {
        self.got.as_str()
    }
}

impl fmt::Display for UnknownWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not {}: expected `{}`",
            self.got, self.what, self.known
        )
    }
}

impl std::error::Error for UnknownWord {}

/// The session a set of cards is chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    /// The role, absent for a local session of the person's own, which takes
    /// every role.
    pub role:  Option<SessionRole>,
    /// Where it runs.
    pub place: Place,
}

impl Session {
    /// A session of no particular role: every card whose place fits it.
    pub const fn every(place: Place) -> Self {
        Self {
            role: None,
            place,
        }
    }

    /// The session the machine's two facts describe.
    ///
    /// `role_file` is the content of [`ROLE_FILE`] when there is one, and `cloud`
    /// is the value of [`CLOUD_VAR`] when it is set. **A file that is there and
    /// holds anything but a role is refused**: reading it as no role would load
    /// every card for a session whose file was merely misspelt.
    pub fn from_parts(role_file: Option<&str>, cloud: Option<&str>) -> Result<Self, UnknownWord> {
        let role = match role_file {
            None => None,
            Some(text) => Some(text.trim().parse()?),
        };
        Ok(Self {
            role,
            place: Place::from_cloud_var(cloud),
        })
    }

    /// Whether a rule binds this session.
    ///
    /// A rule naming no roles binds every role and one naming no place binds
    /// both, so a rule that says nothing is admitted everywhere. A session with
    /// no role is admitted by a rule naming any role.
    pub const fn admits(&self, meta: &RuleMeta) -> bool {
        let role_fits = match self.role {
            None => true,
            Some(role) => {
                let roles = meta.roles.as_slice();
                let mut i = 0;
                let mut found = roles.is_empty();
                while i < roles.len() {
                    if same_role(roles[i], role) {
                        found = true;
                    }
                    i += 1;
                }
                found
            },
        };
        let place_fits = match meta.place {
            None => true,
            Some(p) => same_place(p, self.place),
        };
        role_fits && place_fits
    }
}

/// Whether two roles are the same one. `==` is not yet const on an enum.
const fn same_role(a: SessionRole, b: SessionRole) -> bool {
    matches!(
        (a, b),
        (SessionRole::Coordinator, SessionRole::Coordinator)
            | (SessionRole::Helper, SessionRole::Helper)
    )
}

/// Whether two places are the same one.
const fn same_place(a: Place, b: Place) -> bool {
    matches!(
        (a, b),
        (Place::Cloud, Place::Cloud) | (Place::Local, Place::Local)
    )
}
