//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The engine is not at the top of this repo, and the top of the repo is what
//! anyone reaches for. Reached through the descriptor rather than by calling
//! the function, so a hook that stopped being wired in fails these too.

use std::path::{Path, PathBuf};

fn check(dir: &Path) -> Result<(), String> {
    let hook = homma::TOOL
        .hooks
        .verify_engine_dir
        .expect("the descriptor carries no engine-directory check");
    hook(dir)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the launcher sits under the repo root")
        .to_path_buf()
}

#[test]
fn the_engine_package_itself_is_accepted() {
    let engine = repo_root().join(homma::ENGINE_SUBDIR);
    assert!(
        engine.join("Cargo.toml").is_file(),
        "the engine moved: {}",
        engine.display()
    );
    check(&engine).expect("the real engine directory was refused");
}

#[test]
fn the_repository_root_is_refused_and_told_where_to_go() {
    // Without this, cargo answers with a virtual-manifest complaint that names
    // neither the flag that caused it nor the directory that would have worked.
    let root = repo_root();
    let err = check(&root).expect_err("the repo root was accepted as the engine");
    assert!(
        err.contains(homma::ENGINE_SUBDIR),
        "the refusal does not say where the engine is: {err}"
    );
}

#[test]
fn a_directory_that_is_no_checkout_of_this_is_refused_too() {
    // The control on the one above: refusing the root is about *which* package
    // is there, not about refusing everything that is not the engine.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        b"[package]\nname = \"something-else\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let err = check(dir.path()).expect_err("a foreign package was accepted");
    assert!(
        err.contains(homma::ENGINE_SUBDIR),
        "the refusal does not say where the engine is: {err}"
    );
    assert!(
        !err.contains("is the repository"),
        "a foreign package was reported as this repository: {err}"
    );
}

#[test]
fn a_directory_with_no_manifest_reports_the_manifest_and_not_something_vaguer() {
    let dir = tempfile::tempdir().unwrap();
    let err = check(dir.path()).expect_err("a directory with no manifest was accepted");
    assert!(
        err.contains("Cargo.toml"),
        "the refusal does not name the file it looked for: {err}"
    );
}

fn with_manifest(body: &str) -> Result<(), String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), body).unwrap();
    check(dir.path())
}

#[test]
fn the_engine_name_counts_only_where_a_package_declares_itself() {
    // Every one of these holds the engine's name somewhere a substring scan
    // reads, and none of them is a package `cargo install` would build as the
    // engine. All four were accepted by a scan for the bare text.
    let mentions = [
        (
            "a bin target under the engine's name in some other package",
            "[package]\nname = \"totally-not-homma\"\nversion = \"0.1.0\"\n\n\
             [[bin]]\nname = \"homma-engine\"\npath = \"src/main.rs\"\n",
        ),
        (
            "a comment saying what the file is not",
            "# the engine is name = \"homma-engine\", this is not it\n\
             [package]\nname = \"other\"\nversion = \"0.1.0\"\n",
        ),
        (
            "a renamed dependency on the engine",
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n\
             [dependencies.eng]\nname = \"homma-engine\"\nversion = \"0.1\"\n",
        ),
        (
            "an ordinary dependency on the engine",
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nhomma-engine = \"0.1\"\n",
        ),
    ];
    for (what, manifest) in mentions {
        with_manifest(manifest).expect_err(what);
    }
}

#[test]
fn a_package_declaring_itself_the_engine_is_accepted_however_it_spells_it() {
    // The control on the one above. Refusing everything would pass that test
    // and break the flag entirely, and TOML has three spellings of the same
    // assignment that a scan for one literal string gets wrong.
    for manifest in [
        "[package]\nname = \"homma-engine\"\nversion = \"0.1.0\"\n",
        "[package]\nname=\"homma-engine\"\nversion = \"0.1.0\"\n",
        "[package]\nname = 'homma-engine'\nversion = \"0.1.0\"\n",
        "[package]\nname   =    \"homma-engine\"   # with a trailing comment\n",
    ] {
        with_manifest(manifest).unwrap_or_else(|e| panic!("refused {manifest:?}: {e}"));
    }
}
