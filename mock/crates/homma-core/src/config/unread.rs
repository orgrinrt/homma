//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! An entry of the manifest this engine does not know.
//!
//! The manifest's top level and `[agent]` hold entries that are each a feature
//! of their own, so an entry there under which this engine reads no key is a
//! newer engine's: it is taken out before the strict parse and kept by name.
//! Every other table refuses an unknown key as before, and so do two kinds of
//! entry here: `repos` and `local`, which no engine reads, and one an edit away
//! from a name this engine reads, which is a misspelling.

use std::collections::BTreeSet;

use super::{Config, ConfigError};

mod blank;
mod near;

/// The tables whose entries are each a feature: the top level, named by the
/// empty path, and `[agent]`.
const GROUPS: [&str; 2] = ["", "agent"];

/// Top-level entries refused whatever the key probe says: `repos` is retired
/// with its per-repository settings, and `local` belongs to the local file.
const REFUSED: [&str; 2] = ["repos", "local"];

/// [`Config::parse`]: strict first, and only a manifest that refuses is read
/// again with its unread entries taken out.
pub(super) fn parse(text: &str) -> Result<Config, ConfigError> {
    let refused = match toml::from_str::<Config>(text) {
        Ok(cfg) => return Ok(cfg),
        Err(e) => e,
    };
    let Ok(mut raw) = toml::from_str::<toml::Value>(text) else {
        return Err(ConfigError::Parse(refused));
    };
    // A probe that cannot read the types knows no key, so it cannot tell an
    // entry it does not know from one it does: the refusal stands.
    let Ok(keys) = super::keys() else {
        return Err(ConfigError::Parse(refused));
    };
    let unread = find_unread(&raw, keys.read());
    if unread.is_empty() {
        return Err(ConfigError::Parse(refused));
    }
    // The text itself with the unread entries' lines blanked, so a refusal of
    // what is left names its line.
    let blanked = match toml::from_str::<Config>(&blank::blank(text, &unread)) {
        Ok(mut cfg) => {
            cfg.unread = unread;
            return Ok(cfg);
        },
        Err(e) => e,
    };
    // An entry whose shape the line reading did not follow is taken out of the
    // parsed value; where that refuses as well, the refusal with a line stands.
    take(&mut raw, &unread);
    match raw.try_into::<Config>() {
        Ok(mut cfg) => {
            cfg.unread = unread;
            Ok(cfg)
        },
        Err(_) => Err(ConfigError::Parse(blanked)),
    }
}

/// Every entry of a group table under which `read` lists no key, as a dotted
/// name, sorted: neither one of [`REFUSED`] nor one edit from a name `read`
/// lists in the same table.
fn find_unread(raw: &toml::Value, read: &BTreeSet<String>) -> Vec<String> {
    let mut unread = Vec::new();
    for group in GROUPS {
        let table = if group.is_empty() {
            raw.as_table()
        } else {
            raw.get(group).and_then(toml::Value::as_table)
        };
        let Some(table) = table else { continue };
        let known = names_in(group, read);
        for name in table.keys() {
            let path = if group.is_empty() { name.clone() } else { format!("{group}.{name}") };
            if known.contains(name.as_str()) {
                continue;
            }
            if group.is_empty() && REFUSED.contains(&name.as_str()) {
                continue;
            }
            if known.iter().any(|k| near::one_edit(name, k)) {
                continue;
            }
            unread.push(path);
        }
    }
    unread.sort();
    unread
}

/// The names `read` lists directly under `group`: the first step of each key
/// at the top level, the second of each key under `[agent]`.
fn names_in<'a>(group: &str, read: &'a BTreeSet<String>) -> BTreeSet<&'a str> {
    read.iter()
        .filter_map(|k| {
            if group.is_empty() {
                k.split('.').next()
            } else {
                k.strip_prefix(group)?.strip_prefix('.')?.split('.').next()
            }
        })
        .collect()
}

/// Take each of `unread` out of `raw`.
fn take(raw: &mut toml::Value, unread: &[String]) {
    for path in unread {
        let (group, name) = path.split_once('.').unwrap_or(("", path));
        let table = if group.is_empty() {
            raw.as_table_mut()
        } else {
            raw.get_mut(group).and_then(toml::Value::as_table_mut)
        };
        if let Some(table) = table {
            table.remove(name);
        }
    }
}

impl Config {
    /// What to do about the unread entries, by the manifest's engine pin, or
    /// none where nothing is unread.
    pub fn unread_advice(&self) -> Option<String> {
        if self.unread.is_empty() {
            return None;
        }
        let pinned = [
            ("homma_rev", &self.homma_rev),
            ("homma_tag", &self.homma_tag),
            ("homma_version", &self.homma_version),
        ]
        .into_iter()
        .find_map(|(key, value)| value.as_deref().map(|v| (key, v)));
        let pin = match (pinned, self.homma_branch.as_deref()) {
            (Some((key, value)), _) => {
                format!(
                    "The manifest pins `{key} = \"{value}\"`: move it to an engine that lists them."
                )
            },
            (None, Some(branch)) => {
                format!(
                    "The manifest pins the branch `{branch}`: a run with the network builds its \
                     tip, and an entry that tip does not list either is one no engine of \
                     `{branch}` reads yet, or is misspelt."
                )
            },
            (None, None) => {
                "The manifest pins no engine: set `homma_branch` or `homma_rev` to one that lists \
                 them."
                    .to_string()
            },
        };
        Some(format!(
            "An engine that reads one lists its keys under `reads` in `homma --output json \
             status`. {pin}"
        ))
    }

    /// The unread entries named, and what to do, or none where nothing is
    /// unread.
    pub fn unread_note(&self) -> Option<String> {
        let advice = self.unread_advice()?;
        let names: Vec<String> = self.unread.iter().map(|e| format!("`{e}`")).collect();
        Some(format!(
            "homma.toml has {}, which this engine does not read. {advice}",
            names.join(", ")
        ))
    }

    /// Why a verb reading the whole of `table` stops, where an entry under it
    /// is unread.
    pub fn unread_refusal(&self, table: &str) -> Option<String> {
        let under = format!("{table}.");
        if !self.unread.iter().any(|e| e.starts_with(&under)) {
            return None;
        }
        let note = self.unread_note()?;
        Some(format!(
            "{note} This reads the whole of `[{table}]` and would write what it writes without \
             them, so it stops until then."
        ))
    }
}
