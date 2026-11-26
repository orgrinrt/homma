//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tables of the reading passes: `[search]`, `[summary]` and `[ask]`.

use serde::{Deserialize, Serialize};

use crate::path::RelPath;

/// Where `homma search` finds the indexes it asks, the `[search]` table.
///
/// The content repository's corpora are the `corpus_config` files it tracks, each with its index at
/// the path its own file sets or `corpus_index` beside it. A member's index is `member_index` under
/// its own git directory, and is asked only where `members` says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchConfig {
    /// Whether the members' own indexes are asked beside the content repository's corpora.
    pub members:       bool,
    /// The file that makes a directory of the content repository a corpus.
    pub corpus_config: RelPath,
    /// A corpus's index, relative to its config file's directory, where that file names none.
    pub corpus_index:  RelPath,
    /// A member's index, relative to the member's git directory.
    pub member_index:  RelPath,
    /// The command that builds a corpus's index, run in the corpus's directory; an index not
    /// built is named with it.
    pub corpus_build:  String,
    /// The command that builds a member's index, run in the member's clone; an index not built is
    /// named with it.
    pub member_build:  String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        let rel = |s: &str| RelPath::new(s).expect("a default path is relative and contained");
        Self {
            members:       false,
            corpus_config: rel("muisti.toml"),
            corpus_index:  rel(".muisti/index.sqlite"),
            member_index:  rel("mockspace/search.sqlite"),
            corpus_build:  "muisti index".to_string(),
            member_build:  "cargo mock search --refresh".to_string(),
        }
    }
}

/// The summarising pass, the `[summary]` table: whether it runs, and what makes its call.
///
/// Off unless `enabled` is set, since every summary is a call of a model the workspace pays for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SummaryConfig {
    /// Whether the verbs that offer a summary make one.
    pub enabled: bool,
    /// The command run in print mode for each call.
    pub command: String,
    /// The model the command is asked to use.
    pub model:   String,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "claude".to_string(),
            model:   "haiku".to_string(),
        }
    }
}

/// What `homma ask` keeps, the `[ask]` table: how long an entry lives after its
/// last read, and the size the cache is cut back under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AskConfig {
    /// Hours after its last read an entry expires.
    pub cache_expire_hours: u64,
    /// Bytes the cache is cut back under, least recently read first.
    pub cache_max_bytes:    u64,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self {
            cache_expire_hours: 14 * 24,
            cache_max_bytes:    64 * 1024 * 1024,
        }
    }
}

/// A workspace's own words for `homma what`, the `[what]` table: groups of
/// words that mean the same in that workspace, joined to the compiled table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WhatConfig {
    /// Each group a list of words; empty unless set.
    pub synonyms: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Failing control: without `deny_unknown_fields` the misspelt key reads.
    #[test]
    fn the_ask_table_refuses_an_unknown_key_and_defaults_what_is_absent() {
        let a: AskConfig = toml::from_str("cache_expire_hours = 1\n").unwrap();
        assert_eq!(a.cache_expire_hours, 1);
        assert_eq!(a.cache_max_bytes, AskConfig::default().cache_max_bytes);
        assert!(toml::from_str::<AskConfig>("cache_expiry_hours = 1\n").is_err());
    }

    /// Failing control: without `deny_unknown_fields` the misspelt key reads.
    #[test]
    fn the_what_table_reads_groups_of_words_and_refuses_an_unknown_key() {
        let w: WhatConfig = toml::from_str("synonyms = [[\"a\", \"b\"], [\"c\"]]\n").unwrap();
        assert_eq!(w.synonyms, [vec!["a", "b"], vec!["c"]]);
        assert!(WhatConfig::default().synonyms.is_empty());
        assert!(toml::from_str::<WhatConfig>("synonym = [[\"a\"]]\n").is_err());
    }
}
