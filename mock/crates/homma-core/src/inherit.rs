//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The rule by which a clone's `homma.local.toml` is read over the stored config
//! of its project.
//!
//! Pure over parsed tables: reading the files is [`crate::local`], which calls
//! this once. A table in both is merged key by key, a scalar in both takes the
//! local value, an array in both is replaced whole by the local one, a key in
//! one only is kept, and a value of one kind over a value of another takes the
//! local one whole. `unset` removes paths from the base before the local file is
//! applied. Alongside the merged table comes a [`Provenance`], which says for
//! every leaf (a scalar or an array) which side it came from.

use serde::Serialize;
use toml::{Table, Value};

/// The two keys that are the mechanism of inheriting and not part of the config
/// being inherited. Neither is removable by `unset`, and neither is counted as
/// a value the file sets.
pub const PROJECT_PATH: &str = "instance.project";
pub const UNSET_PATH: &str = "instance.unset";

/// Where each leaf of a merged config came from, as dotted paths, each list
/// sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Provenance {
    /// The project the base is the stored config of.
    pub project:    String,
    /// Only in the base, so kept as it is.
    pub inherited:  Vec<String>,
    /// Only in the local file, so added to the base.
    pub local:      Vec<String>,
    /// In both, and the local value is the one in force.
    pub overridden: Vec<String>,
    /// Removed from the base by `unset`, each as written, where the base had it.
    pub unset:      Vec<String>,
}

/// A path in `unset` that cannot be removed or is not a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadUnset(pub String);

/// Merge `local` over `base`, after removing each path in `unset` from `base`.
pub fn merge(
    project: &str,
    base: &Table,
    local: &Table,
    unset: &[String],
) -> Result<(Table, Provenance), BadUnset> {
    let mut base = base.clone();
    let mut prov = Provenance {
        project: project.to_owned(),
        ..Provenance::default()
    };
    for path in unset {
        if path.is_empty() || path.split('.').any(str::is_empty) || is_mechanism(path) {
            return Err(BadUnset(path.clone()));
        }
        if remove(&mut base, path) {
            prov.unset.push(path.clone());
        }
    }
    let merged = merge_tables(&base, local, "", &mut prov);
    prov.inherited.sort();
    prov.local.sort();
    prov.overridden.sort();
    prov.unset.sort();
    Ok((merged, prov))
}

fn is_mechanism(path: &str) -> bool {
    path == PROJECT_PATH || path == UNSET_PATH || path == "instance"
}

/// Remove the value at the dotted `path`, `true` where there was one.
fn remove(table: &mut Table, path: &str) -> bool {
    match path.split_once('.') {
        None => table.remove(path).is_some(),
        Some((head, rest)) => {
            match table.get_mut(head) {
                Some(Value::Table(inner)) => remove(inner, rest),
                _ => false,
            }
        },
    }
}

fn merge_tables(base: &Table, local: &Table, at: &str, prov: &mut Provenance) -> Table {
    let mut out = Table::new();
    for (key, value) in base {
        if !local.contains_key(key) {
            leaves(value, &join(at, key), &mut prov.inherited);
            out.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in local {
        let path = join(at, key);
        match (base.get(key), value) {
            (Some(Value::Table(b)), Value::Table(l)) => {
                out.insert(key.clone(), Value::Table(merge_tables(b, l, &path, prov)));
            },
            (Some(_), _) => {
                leaves(value, &path, &mut prov.overridden);
                out.insert(key.clone(), value.clone());
            },
            (None, _) => {
                leaves(value, &path, &mut prov.local);
                out.insert(key.clone(), value.clone());
            },
        }
    }
    out
}

fn join(at: &str, key: &str) -> String {
    if at.is_empty() { key.to_owned() } else { format!("{at}.{key}") }
}

/// Push the dotted path of each leaf under `value`, skipping the mechanism.
fn leaves(value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Table(t) if !t.is_empty() => {
            for (key, inner) in t {
                leaves(inner, &join(path, key), out);
            }
        },
        _ if is_mechanism(path) => {},
        _ => out.push(path.to_owned()),
    }
}

#[cfg(test)]
#[path = "inherit_tests.rs"]
mod tests;
