//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! A workspace's name, which becomes a directory name.
//!
//! A session's role file lives in a directory named for the workspace, so the
//! name is refused when the manifest is read unless it is one plain path
//! component: never empty, never `.` or `..` or starting with a dot, and with no
//! path separator or control character.

use std::fmt;

use serde::{Deserialize, Serialize};

/// `[workspace] name`, checked as one plain path component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspaceName(String);

/// Why a string cannot be a workspace's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAWorkspaceName {
    pub name: String,
    pub why:  &'static str,
}

impl fmt::Display for NotAWorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the workspace name {:?} cannot be used: {}",
            self.name, self.why
        )
    }
}

impl std::error::Error for NotAWorkspaceName {}

impl WorkspaceName {
    pub fn new(name: impl Into<String>) -> Result<Self, NotAWorkspaceName> {
        let name = name.into();
        let why = if name.is_empty() {
            Some("it is empty")
        } else if name.contains('/') || name.contains('\\') {
            Some("it carries a path separator, and it becomes one directory's name")
        } else if name.starts_with('.') {
            Some("it starts with a dot, which names a hidden or a parent directory")
        } else if name.chars().any(char::is_control) {
            Some("it carries a control character")
        } else {
            None
        };
        match why {
            Some(why) => {
                Err(NotAWorkspaceName {
                    name,
                    why,
                })
            },
            None => Ok(Self(name)),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkspaceName {
    type Error = NotAWorkspaceName;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::new(name)
    }
}

impl From<WorkspaceName> for String {
    fn from(name: WorkspaceName) -> Self {
        name.0
    }
}

impl PartialEq<str> for WorkspaceName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for WorkspaceName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl fmt::Display for WorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_is_taken_whole() {
        assert_eq!(
            WorkspaceName::new("glade-root").unwrap().as_str(),
            "glade-root"
        );
        assert_eq!(WorkspaceName::new("työmaa").unwrap().as_str(), "työmaa");
    }

    #[test]
    fn a_name_that_is_not_one_plain_component_is_refused() {
        for bad in ["", ".", "..", ".hidden", "a/b", "a\\b", "../up", "a\nb"] {
            assert!(WorkspaceName::new(bad).is_err(), "{bad:?} was taken");
        }
    }
}
