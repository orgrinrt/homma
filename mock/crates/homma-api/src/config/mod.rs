//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The registry: who exists, and what each of them needs to act.
//!
//! Identity is a configuration entry, never a running process. A participant
//! outlives whatever was running it, so starting one again restores a
//! participant rather than creating a new one.

pub mod identity;
pub mod paths;
pub mod tables;
pub mod unreadable;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use self::identity::{Identity, Role};
use self::paths::Paths;
use self::unreadable::unsafe_strings_of;

/// The literal meaning "this directory is the content repository".
pub const LOCAL: &str = "local";

fn local() -> String {
    LOCAL.to_string()
}

/// What homma needs of a workspace, all of which has a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Where the repository holding workspace metadata and content lives.
    ///
    /// A git URI, or the literal `local`, which is the default.
    ///
    /// A URI rather than a name, because it is what a workspace is cloned from.
    /// A name would have to be resolved into one, and the round that tried
    /// resolved it from whatever repository the operator happened to be standing
    /// in, which cloned that repository instead.
    ///
    /// `local` means the configuration's own directory **is** the content
    /// repository, initialised if it is not one yet. That is the shape a
    /// workspace starts in before it has a remote, so it is the default and
    /// needs no key at all.
    #[serde(default = "local")]
    pub content_repo: String,
    #[serde(default)]
    pub paths:        Paths,
    /// Handle to entry.
    #[serde(default)]
    pub org:          BTreeMap<String, Identity>,
    /// Office to its row: the standing jobs pinged by name, each with at most
    /// one holder from `org`.
    #[serde(default)]
    pub offices:      BTreeMap<String, crate::Office>,
    /// Where the lease records live and how long one lasts.
    #[serde(default)]
    pub lease:        crate::LeaseConfig,
    /// Places this operator's homma may not write, beyond the ones it derives.
    ///
    /// Two locations are denied wherever homma runs, because they are the same
    /// everywhere: the operator's own `~/.claude`, and every other
    /// participant's workspace as the registry gives them. Anything else is one
    /// operator's arrangement and is named here rather than compiled in.
    #[serde(default)]
    pub deny:         Vec<crate::DenyEntry>,
}

impl Workspace {
    /// The role a handle holds, for deriving a record's standing.
    pub fn role_of(&self, handle: &str) -> Option<Role> {
        self.org.get(handle).map(|i| i.role)
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Registry strings that would inject structure into anything generated
    /// from them.
    ///
    /// A generated agent definition puts these into YAML frontmatter, so a
    /// newline in a nickname writes an arbitrary key. That is how a Hand could
    /// grant its own twin the memory key the design withholds structurally, by
    /// editing its own entry in a file every participant can write.
    pub fn unsafe_strings(&self) -> Vec<(String, String)> {
        unsafe_strings_of(&self.org)
    }
}

#[cfg(test)]
const MINIMAL: &str = r#"
content_repo = "git@example.invalid:someone/content.git"
"#;

#[cfg(test)]
const WITH_ORG: &str = r#"
content_repo = "git@example.invalid:someone/content.git"

[paths]
hands = "custom/hands"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
staffed = true
handle = "paja"
nickname = "Paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"
repos = ["homma"]

[org.rendering]
role = "hand"
handle = "rendering"
domain = "rendering"

[org.proof]
role = "expert"
handle = "proof"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_key_is_enough() {
        let w = Workspace::parse(MINIMAL).expect("should parse");
        assert_eq!(w.content_repo, "git@example.invalid:someone/content.git");
        assert!(w.org.is_empty());
    }

    #[test]
    fn the_registry_answers_what_role_a_handle_holds() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.role_of("op"), Some(Role::King));
        assert_eq!(w.role_of("paja"), Some(Role::Hand));
        assert_eq!(w.role_of("stranger"), None);
    }
}
