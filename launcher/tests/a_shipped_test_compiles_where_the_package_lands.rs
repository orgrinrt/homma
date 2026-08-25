//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A test named in `include` does not import a crate cargo strips on publish.
//!
//! Cargo drops a dev-dependency that carries a path and no version, because
//! there is nothing for the registry to resolve. Everything still builds from
//! a checkout, where the path is right there, and `cargo publish` does not
//! notice either: verification builds the lib and the bins and no test at all.
//! The first person to meet it is whoever unpacks the tarball and runs its
//! suite, against a version that cannot be replaced.
//!
//! So this reads the manifest twice and compares. What `include` ships, and
//! what the tarball will not have.

use std::collections::BTreeSet;
use std::path::Path;

/// The dependencies cargo strips: named by path, carrying no version, under
/// `[dev-dependencies]`.
///
/// A regular or build dependency of that shape is refused outright at publish
/// rather than stripped, so it is a different defect and not this one's.
fn stripped_dev_deps(manifest: &str) -> BTreeSet<String> {
    let Some((_, rest)) = manifest.split_once("[dev-dependencies]") else {
        return BTreeSet::new();
    };
    rest.split("\n[")
        .next()
        .unwrap_or(rest)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, spec) = line.split_once('=')?;
            (spec.contains("path") && !spec.contains("version"))
                .then(|| name.trim().replace('-', "_"))
        })
        .collect()
}

/// The `tests/…` entries `include` names, without globs.
fn shipped_tests(manifest: &str) -> Vec<String> {
    let Some((_, rest)) = manifest.split_once("include = [") else {
        return Vec::new();
    };
    rest.split_once(']')
        .map(|(inside, _)| inside)
        .unwrap_or(rest)
        .split('"')
        .filter(|s| s.starts_with("tests/") && s.ends_with(".rs"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_shipped_test_imports_a_dependency_the_package_will_not_carry() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("no manifest");

    let stripped = stripped_dev_deps(&manifest);
    let shipped = shipped_tests(&manifest);

    // Either side empty makes the loop below vacuous, and a vacuous pass reads
    // exactly like a real one.
    assert!(
        !shipped.is_empty(),
        "`include` names no test files, so this check ran over nothing. Either \
         the manifest stopped naming them or the parse stopped finding them."
    );
    assert!(
        !stripped.is_empty(),
        "no dev-dependency is stripped on publish, so this check ran over \
         nothing. That is a fine state to be in and it is not this test's job \
         to assert it: delete this test rather than leaving it green over an \
         empty set."
    );

    for name in &shipped {
        let body = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|e| panic!("{name} is in `include` and not on disk: {e}"));
        for dep in &stripped {
            assert!(
                !body.contains(dep.as_str()),
                "{name} ships and names `{dep}`, which cargo strips on publish. \
                 Drop it from `include`, or give the dependency a version so it \
                 survives."
            );
        }
    }
}

#[test]
fn the_check_sees_the_defect_it_exists_for() {
    // The manifest as it was before the fix: the pin test shipping, and
    // `homma-core` reachable only by path.
    let before = r#"
        [package]
        include = ["src/**/*.rs", "tests/the_pin_survives_the_engine.rs"]

        [dev-dependencies]
        homma-core = { path = "../mock/crates/homma-core" }
        tempfile = "3"
    "#;
    assert_eq!(
        stripped_dev_deps(before),
        BTreeSet::from(["homma_core".to_string()]),
        "the underscore spelling is what a `use` writes, and matching the \
         hyphenated one would find nothing in any test file"
    );
    assert_eq!(shipped_tests(before), vec![
        "tests/the_pin_survives_the_engine.rs"
    ]);

    // And a versioned path dependency is not stripped, so it is not reported.
    let versioned = r#"
        [dev-dependencies]
        sibling = { path = "../sibling", version = "0.0.1" }
    "#;
    assert!(stripped_dev_deps(versioned).is_empty());
}
