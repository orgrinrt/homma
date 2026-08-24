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

#[test]
fn a_package_merely_mentioning_the_engine_is_not_the_engine() {
    // A consumer's manifest names `homma-engine` in its dependency table, and
    // a check looking for the bare word would take it for the engine itself.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        b"[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n\
          [dependencies]\nhomma-engine = \"0.1\"\n",
    )
    .unwrap();
    check(dir.path()).expect_err("a consumer of the engine was taken for the engine");
}
