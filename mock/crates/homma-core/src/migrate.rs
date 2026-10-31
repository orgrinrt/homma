//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Turning a full copy of a stored config into a file that inherits it.

use crate::inherit;
use crate::local::{Local, LocalError};

/// What [`migrate`] made of a copy.
#[derive(Debug, PartialEq, Eq)]
pub struct Migrated {
    /// The new text of the file.
    pub text:    String,
    /// Dotted paths of the values dropped because they equalled the stored
    /// config's, and are inherited from now on.
    pub dropped: Vec<String>,
    /// Dotted paths of the values kept, each an override of the stored config
    /// or an addition to it.
    pub kept:    Vec<String>,
}

/// Turn a full copy of a stored config into a file that inherits it.
///
/// The `project` line is added, every value equal to the stored one is dropped
/// along with the tables that empties, and every other value stays, so it is an
/// override or an addition. A value the stored config has and the copy lacks is
/// inherited from then on. The one removal the copy implies is a dashboard its
/// console replaced: a console and a stored dashboard together are refused, so
/// the file then unsets `tools.dashboard`. Comments around what stays are kept.
pub fn migrate(existing: &str, stored: &str, project: &str) -> Result<Migrated, LocalError> {
    let mut doc: toml_edit::DocumentMut = existing.parse().map_err(LocalError::Edit)?;
    let base: toml::Table = toml::from_str(stored).map_err(LocalError::Refused)?;
    let copy = Local::parse(existing).map_err(LocalError::Refused)?;
    let mut dropped = Vec::new();
    drop_equal(doc.as_table_mut(), &base, "", &mut dropped);

    let replaced_dashboard = copy.names_console()
        && !copy.tools.contains_key("dashboard")
        && base
            .get("tools")
            .and_then(toml::Value::as_table)
            .is_some_and(|t| t.contains_key("dashboard"));
    let instance = doc
        .entry("instance")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| LocalError::Key("instance".to_owned()))?;
    instance.insert("project", toml_edit::value(project));
    if replaced_dashboard {
        let mut unset = toml_edit::Array::new();
        unset.push("tools.dashboard");
        instance.insert("unset", toml_edit::value(unset));
    }
    let text = doc.to_string();
    Local::parse(&text).map_err(LocalError::Refused)?;
    let mut kept = Vec::new();
    leaves(doc.as_item(), "", &mut kept);
    kept.retain(|p| p != inherit::PROJECT_PATH && p != inherit::UNSET_PATH);
    Ok(Migrated {
        text,
        dropped,
        kept,
    })
}

fn drop_equal(
    into: &mut dyn toml_edit::TableLike,
    base: &toml::Table,
    at: &str,
    dropped: &mut Vec<String>,
) {
    let keys: Vec<String> = into.iter().map(|(k, _)| k.to_owned()).collect();
    for key in keys {
        let Some(b) = base.get(&key) else { continue };
        let path = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
        let Some(item) = into.get_mut(&key) else { continue };
        let both_tables = item.as_table_like().is_some() && b.is_table();
        if both_tables {
            let (Some(inner), Some(bt)) = (item.as_table_like_mut(), b.as_table()) else {
                continue;
            };
            drop_equal(inner, bt, &path, dropped);
            // The instance table stays even when empty: it carries the file's
            // header comment and is where the project line goes.
            if inner.is_empty() && path != "instance" {
                into.remove(&key);
            }
        } else if item_value(item).as_ref() == Some(b) {
            dropped.push(path);
            into.remove(&key);
        }
    }
}

/// A value or array-of-tables item as the typed value it parses to.
fn item_value(item: &toml_edit::Item) -> Option<toml::Value> {
    // An array of tables reads as the array of inline tables it is, so a copied
    // `[[hooks.x]]` equal to the store's compares equal.
    let value = item.clone().into_value().ok()?;
    let mut parsed: toml::Table = toml::from_str(&format!("v = {value}")).ok()?;
    parsed.remove("v")
}

fn leaves(item: &toml_edit::Item, path: &str, out: &mut Vec<String>) {
    match item.as_table_like() {
        Some(t) if !t.is_empty() => {
            for (key, inner) in t.iter() {
                let next = if path.is_empty() { key.to_owned() } else { format!("{path}.{key}") };
                leaves(inner, &next, out);
            }
        },
        _ => out.push(path.to_owned()),
    }
}
