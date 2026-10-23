//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! renki builds on unix only, so the engine reaches it, and the two crates it is read with, on unix only.
//!
//! The dependencies are read off the manifest, because a target that is not installed cannot be built
//! from here: what holds for it is that nothing it builds names a crate that does not build there.

use toml::Table;

const MANIFEST: &str = include_str!("../Cargo.toml");
const UNIX_ONLY: [&str; 3] = ["renki", "renki-dirs", "notko"];

fn manifest() -> Table {
    MANIFEST.parse().expect("the manifest is toml")
}

fn names(t: &Table, path: &[&str]) -> Vec<String> {
    let mut at = t;
    for key in path {
        match at.get(*key).and_then(|v| v.as_table()) {
            Some(next) => at = next,
            None => return Vec::new(),
        }
    }
    at.keys().cloned().collect()
}

#[test]
fn no_target_carries_renki_or_what_it_is_read_with_unless_it_is_unix() {
    let m = manifest();
    let everywhere = names(&m, &["dependencies"]);
    for dep in UNIX_ONLY {
        assert!(
            !everywhere.iter().any(|d| d == dep),
            "`{dep}` is a dependency on every target, and it does not build on all of them"
        );
    }
}

#[test]
fn the_unix_targets_carry_all_three() {
    let m = manifest();
    let unix = names(&m, &["target", "cfg(unix)", "dependencies"]);
    for dep in UNIX_ONLY {
        assert!(
            unix.iter().any(|d| d == dep),
            "`{dep}` is not under `[target.'cfg(unix)'.dependencies]`"
        );
    }
}

#[test]
fn no_other_target_table_names_them() {
    let m = manifest();
    let Some(targets) = m.get("target").and_then(|t| t.as_table()) else {
        return;
    };
    for (cfg, t) in targets {
        if cfg == "cfg(unix)" {
            continue;
        }
        let deps = names(t.as_table().unwrap(), &["dependencies"]);
        for dep in UNIX_ONLY {
            assert!(!deps.iter().any(|d| d == dep), "`{dep}` under {cfg}");
        }
    }
}
