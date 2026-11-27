//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The engine and the launcher take one renki, so one is built for both and a fix reaches both.
//!
//! Read off the two lockfiles, the engine's workspace's and the launcher's, since what is built is
//! what they resolved and not what a manifest asks for.

use toml::Table;

const ENGINE_LOCK: &str = include_str!("../../../Cargo.lock");
const LAUNCHER_LOCK: &str = include_str!("../../../../Cargo.lock");
const SHARED: [&str; 2] = ["renki", "renki-dirs"];

/// Each `version source` a lock resolves `name` to.
fn resolved(lock: &str, name: &str) -> Vec<String> {
    let lock: Table = lock.parse().expect("a lockfile is toml");
    lock.get("package")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .map(|p| {
            let field = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            format!("{} {}", field("version"), field("source"))
        })
        .collect()
}

#[test]
fn the_engine_and_the_launcher_resolve_one_renki() {
    for name in SHARED {
        let launcher = resolved(LAUNCHER_LOCK, name);
        assert_eq!(
            launcher.len(),
            1,
            "the launcher resolves `{name}` once: {launcher:?}"
        );
        assert_eq!(
            resolved(ENGINE_LOCK, name),
            launcher,
            "the engine resolves `{name}` apart from the launcher"
        );
    }
}

#[test]
fn the_check_tells_two_renkis_apart() {
    // Failing control: the same name at another version or source reads as another renki.
    let one = "[[package]]\nname = \"renki\"\nversion = \"0.0.3\"\nsource = \"registry+x\"\n";
    let other = "[[package]]\nname = \"renki\"\nversion = \"0.0.5\"\nsource = \"git+y\"\n";
    assert_ne!(resolved(one, "renki"), resolved(other, "renki"));
    assert_eq!(resolved(one, "renki"), ["0.0.3 registry+x"]);
}

// The launcher owns the rule that renki is required exactly, and the engine takes the same renki,
// so its manifest is held to the same predicate, read from the launcher's tests by path.
#[path = "../../../../launcher/tests/support/renki_requirements.rs"]
mod renki_requirements;

const ENGINE_MANIFEST: &str = include_str!("../Cargo.toml");

#[test]
fn the_engine_takes_its_renki_crates_exactly() {
    assert!(
        renki_requirements::names_renki(ENGINE_MANIFEST),
        "the engine stopped depending on renki, so the check below would pass over an empty set"
    );
    let loose = renki_requirements::loose_renki(ENGINE_MANIFEST);
    assert!(
        loose.is_empty(),
        "a renki requirement that is not `=x.y.z` takes newer releases than the launcher's: \
         {loose:#?}"
    );
}

#[test]
fn the_engine_check_flags_a_caret_requirement_under_the_unix_target() {
    // Failing control: the shape the engine's manifest has, in its loose form.
    let caret =
        "[target.'cfg(unix)'.dependencies]\nrenki = \"0.0.5-rc.4\"\nrenki-dirs = \"0.0.2\"\n";
    assert_eq!(renki_requirements::loose_renki(caret), vec![
        ("renki".to_string(), "0.0.5-rc.4".to_string()),
        ("renki-dirs".to_string(), "0.0.2".to_string()),
    ]);
}
