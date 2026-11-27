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
