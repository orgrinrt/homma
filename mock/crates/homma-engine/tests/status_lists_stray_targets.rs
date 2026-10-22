//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma status` and the build directories other than the workspace's own,
//! through the binary.
//!
//! The unit tests hold the walk. What they cannot say is that the status runs
//! it over the workspace root and prints what it found, in both renderings.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("homma.toml"),
        "content_repo = \"c\"\n\n[workspace]\nname = \"w\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("target/debug")).unwrap();
    dir
}

#[test]
fn a_worktrees_own_target_is_listed_with_its_size() {
    let dir = workspace();
    let stray = dir.path().join(".worktrees/w/mock/target/debug");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("big"), vec![0u8; 3 * 1024]).unwrap();

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "build directories other than the workspace's one target/:",
        ))
        .stdout(predicate::str::contains("3 KiB"))
        .stdout(predicate::str::contains(".worktrees/w/mock/target"));
}

#[test]
fn the_document_carries_them_too() {
    let dir = workspace();
    std::fs::create_dir_all(dir.path().join("repo/target/debug")).unwrap();

    let out = bin()
        .args(["--dir", dir.path().to_str().unwrap(), "--output", "json", "status"])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let listed = doc["stray_targets"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0]["path"].as_str().unwrap().ends_with("repo/target"));
}

#[test]
fn the_workspaces_one_target_alone_prints_nothing_about_it() {
    let dir = workspace();

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("build directories").not());
}
