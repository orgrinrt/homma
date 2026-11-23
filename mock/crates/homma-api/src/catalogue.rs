//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The catalogue's rows beside the identities: the offices and the lease's
//! table.
//!
//! An office is a standing job pinged by its name, held by one person of the
//! catalogue; the lease records which session holds an identity now. Both are
//! configuration read from the same file as the `[org.*]` rows, so the
//! catalogue stays one source.

use serde::{Deserialize, Serialize};

/// One office, `[offices.<name>]`.
///
/// The office's name is its key, so one office has at most one holder by
/// construction: a TOML key cannot repeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Office {
    /// The handle of whoever holds it. Absent, the office exists and waits for
    /// its person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder:  Option<String>,
    /// Whether the office is in the count of coordinator slots. False for one
    /// that stands beside the others permanently.
    #[serde(default = "counted")]
    pub counted: bool,
}

fn counted() -> bool {
    true
}

/// The `[lease]` table: where the lease records live and how long one lasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LeaseConfig {
    /// The remote the records are pushed to.
    pub remote:  String,
    /// The branch on it that holds them: the workspace repository's trunk.
    pub branch:  String,
    /// How long a lease lasts after it is taken or renewed, in minutes.
    pub minutes: u32,
}

impl Default for LeaseConfig {
    fn default() -> Self {
        Self {
            remote:  "origin".to_string(),
            branch:  "main".to_string(),
            minutes: 120,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_office_is_counted_unless_it_says_otherwise() {
        let o: Office = toml::from_str("holder = \"onni\"").unwrap();
        assert!(o.counted);
        assert_eq!(o.holder.as_deref(), Some("onni"));
        let o: Office = toml::from_str("counted = false").unwrap();
        assert!(!o.counted);
        assert_eq!(o.holder, None);
    }

    #[test]
    fn the_lease_table_defaults_to_the_trunk_and_two_hours() {
        let l: LeaseConfig = toml::from_str("").unwrap();
        assert_eq!(l, LeaseConfig::default());
        assert_eq!(
            (l.remote.as_str(), l.branch.as_str(), l.minutes),
            ("origin", "main", 120)
        );
        let l: LeaseConfig = toml::from_str("minutes = 30").unwrap();
        assert_eq!(l.minutes, 30);
        assert_eq!(l.branch, "main");
    }
}
