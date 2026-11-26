//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The vocabulary a query is matched in: the compiled synonym table joined with a workspace's own
//! groups from `[what] synonyms`, groups that share a word once stemmed being one group.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use homma_api::WhatConfig;
use homma_core::Config;

use super::text::{SYNONYMS, stem};

/// Which group each stem belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    group: HashMap<String, usize>,
}

impl Vocabulary {
    /// The compiled table alone.
    pub fn compiled() -> &'static Vocabulary {
        static COMPILED: OnceLock<Vocabulary> = OnceLock::new();
        COMPILED.get_or_init(|| Vocabulary::with(&[]))
    }

    /// The compiled table joined with `own`, a workspace's groups.
    pub fn with(own: &[Vec<String>]) -> Vocabulary {
        let groups: Vec<Vec<String>> = SYNONYMS
            .iter()
            .map(|g| g.iter().map(|w| stem(w)).collect())
            .chain(own.iter().map(|g| g.iter().map(|w| stem(w)).collect()))
            .collect();
        let mut parent: Vec<usize> = (0 .. groups.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut first: HashMap<&str, usize> = HashMap::new();
        for (i, g) in groups.iter().enumerate() {
            for w in g {
                match first.get(w.as_str()) {
                    Some(&j) => {
                        let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                        parent[a.max(b)] = a.min(b);
                    },
                    None => {
                        first.insert(w, i);
                    },
                }
            }
        }
        let group = first
            .into_iter()
            .map(|(w, i)| (w.to_string(), root(&mut parent, i)))
            .collect();
        Vocabulary {
            group,
        }
    }

    /// The group a stem belongs to, if any.
    pub fn group_of(&self, stem_word: &str) -> Option<usize> {
        self.group.get(stem_word).copied()
    }

    /// The vocabulary of a workspace whose configuration is `cfg`: the compiled table joined with
    /// its `[what] synonyms`. With no configuration, or no `[what]`, the compiled table.
    ///
    /// # Errors
    /// A `[what]` table that does not read as one, a key it does not know among them.
    pub fn of(cfg: Option<&Config>) -> Result<Vocabulary> {
        let Some(what) = cfg.and_then(|c| c.what.as_ref()) else {
            return Ok(Vocabulary::compiled().clone());
        };
        let read: WhatConfig = what.clone().try_into().context("reading `[what]`")?;
        Ok(Vocabulary::with(&read.synonyms))
    }
}
