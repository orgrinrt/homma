//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Whether a manifest requires the renki crates exactly.
//!
//! The one predicate behind two tests: the launcher's, which owns the rule, and the engine's, which
//! takes the same renki and reads this file by path. A requirement is exact when it starts with
//! `=`; a bare `0.0.5-rc.4` is a caret requirement and takes newer releases too.

/// Every `renki*` dependency whose requirement is not exact, with the requirement it names.
///
/// Read from every dependency table of the manifest, the target-specific ones included, since the
/// engine names renki only under `cfg(unix)`. A dependency naming no `version` is loose too, since
/// cargo then takes whatever the source offers.
pub fn loose_renki(text: &str) -> Vec<(String, String)> {
    let doc: toml::Table = text.parse().expect("the manifest does not parse");
    let mut tables: Vec<&toml::Table> = Vec::new();
    let kinds = ["dependencies", "build-dependencies", "dev-dependencies"];
    for kind in kinds {
        tables.extend(doc.get(kind).and_then(toml::Value::as_table));
    }
    for target in doc
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|t| t.values())
    {
        for kind in kinds {
            tables.extend(target.get(kind).and_then(toml::Value::as_table));
        }
    }

    let mut found = Vec::new();
    for deps in tables {
        for (name, spec) in deps {
            if !name.starts_with("renki") {
                continue;
            }
            let req = match spec {
                toml::Value::String(req) => req.as_str(),
                other => {
                    other
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("")
                },
            };
            if !req.trim_start().starts_with('=') {
                found.push((name.clone(), req.to_string()));
            }
        }
    }
    found
}

/// A manifest that names `renki` at all, in any dependency table.
pub fn names_renki(text: &str) -> bool {
    let doc: toml::Table = text.parse().expect("the manifest does not parse");
    let has = |t: &toml::Table| {
        ["dependencies", "build-dependencies", "dev-dependencies"]
            .iter()
            .any(|k| {
                t.get(*k)
                    .and_then(toml::Value::as_table)
                    .is_some_and(|d| d.contains_key("renki"))
            })
    };
    has(&doc)
        || doc
            .get("target")
            .and_then(toml::Value::as_table)
            .is_some_and(|t| t.values().filter_map(toml::Value::as_table).any(has))
}
