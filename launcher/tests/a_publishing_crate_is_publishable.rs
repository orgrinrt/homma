//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A crate marked `publish = true` names no dependency crates.io will refuse.
//!
//! Two shapes are refused and they are one class. A dependency pinned to a git
//! repository, which the registry does not take. And a dependency named by
//! path with no `version`, which cargo strips on publish and is then left with
//! nothing to resolve. A path-only *dev*-dependency is fine, because that one
//! is stripped whole.
//!
//! The refusal arrives at `cargo publish` and nowhere earlier, so a manifest
//! carrying either is green on every local check and fails at the one step
//! that cannot be retried against a version already taken.
//!
//! The same manifest also holds the launcher to the renki it was built
//! against: every `renki*` requirement is an exact one, `=x.y.z`. A bare
//! `0.0.5-rc.4` is a caret requirement and takes rc.5 and 0.0.5 as well, which
//! moves what every operator installs without the number here moving. The
//! predicate is in `support/renki_requirements.rs`, which the engine's
//! `one_renki` test reads by path to hold its own manifest to the same rule.

use std::path::Path;

// The predicate is the engine's test's as well, which reads this file by path.
#[path = "support/renki_requirements.rs"]
mod renki_requirements;
use renki_requirements::{loose_renki, names_renki};

/// Why the registry would refuse a dependency.
#[derive(Debug, PartialEq)]
enum Why {
    Git,
    PathWithoutVersion,
}

/// Every dependency crates.io would refuse, in a manifest that publishes.
fn refused(text: &str) -> Vec<(String, Why)> {
    let package = text.split("\n[").next().unwrap_or(text);
    if !package.lines().any(|l| l.trim() == "publish = true") {
        return Vec::new();
    }

    let mut found = Vec::new();
    for table in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let Some((_, rest)) = text.split_once(&format!("\n[{table}]")) else {
            continue;
        };
        let section = rest.split("\n[").next().unwrap_or(rest);
        for line in section.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, spec)) = line.split_once('=') else {
                continue;
            };
            let name = name.trim().to_string();
            if spec.contains("git") {
                found.push((name, Why::Git));
            } else if spec.contains("path")
                && !spec.contains("version")
                && table != "dev-dependencies"
            {
                found.push((name, Why::PathWithoutVersion));
            }
        }
    }
    found
}

#[test]
fn this_launcher_is_publishable() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("no manifest");
    assert!(
        manifest.contains("publish = true"),
        "this crate stopped publishing, so the check below would pass over an \
         empty set and say nothing"
    );
    let offenders = refused(&manifest);
    assert!(
        offenders.is_empty(),
        "this crate publishes and names a dependency crates.io refuses at \
         `cargo publish`. Give it a registry version, or set `publish = false` \
         until it has one: {offenders:#?}"
    );
}

#[test]
fn the_check_sees_both_shapes_and_neither_false_positive() {
    let git =
        refused("[package]\npublish = true\n\n[dependencies]\nx = { git = \"ssh://e/x.git\" }\n");
    assert_eq!(git, vec![("x".to_string(), Why::Git)]);

    let pathless =
        refused("[package]\npublish = true\n\n[dependencies]\nx = { path = \"../x\" }\n");
    assert_eq!(pathless, vec![("x".to_string(), Why::PathWithoutVersion)]);

    // A versioned path resolves against the registry, and a path-only
    // dev-dependency is stripped whole. Neither is refused.
    assert!(
        refused(
            "[package]\npublish = true\n\n[dependencies]\nx = { path = \"../x\", version = \"1\" \
             }\n\n[dev-dependencies]\ny = { path = \"..\" }\n"
        )
        .is_empty()
    );

    // And a crate that does not publish may name anything.
    assert!(
        refused("[package]\npublish = false\n\n[dependencies]\nx = { git = \"ssh://e/x.git\" }\n")
            .is_empty()
    );
}

#[test]
fn this_launcher_takes_its_renki_crates_exactly() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("no manifest");
    assert!(
        names_renki(&manifest),
        "the launcher stopped depending on renki, so the check below would pass over an empty set"
    );
    let loose = loose_renki(&manifest);
    assert!(
        loose.is_empty(),
        "a renki requirement that is not `=x.y.z` takes newer releases too, and the launcher \
         moves with them: {loose:#?}"
    );
}

#[test]
fn the_check_tells_an_exact_renki_from_a_caret_one() {
    // Failing control: the caret and bare forms are loose, the `=` form is not.
    let caret = "[dependencies]\nrenki = \"0.0.5-rc.4\"\nrenki-config = { version = \"0.0.2\" }\n";
    assert_eq!(loose_renki(caret), vec![
        ("renki".to_string(), "0.0.5-rc.4".to_string()),
        ("renki-config".to_string(), "0.0.2".to_string()),
    ]);
    let exact = "[dependencies]\nrenki = \"=0.0.5-rc.4\"\nrenki-config = { version = \"=0.0.2\" }\n\
                 notko = \"0.0.1\"\n";
    assert!(loose_renki(exact).is_empty());
    // A renki source with no version names nothing exact.
    let bare = "[dependencies]\nrenki = { path = \"../renki\" }\n";
    assert_eq!(loose_renki(bare), vec![(
        "renki".to_string(),
        String::new()
    )]);
    // A target-specific table is read too, which is where the engine names renki.
    let target = "[target.'cfg(unix)'.dependencies]\nrenki = \"0.0.5-rc.4\"\n";
    assert_eq!(loose_renki(target), vec![(
        "renki".to_string(),
        "0.0.5-rc.4".to_string()
    )]);
    assert!(names_renki(target));
    assert!(!names_renki("[dependencies]\nnotko = \"0.0.1\"\n"));
}
