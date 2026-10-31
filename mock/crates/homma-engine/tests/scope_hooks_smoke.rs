//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A clone outside the instance's scope is not a member, and its git hooks still run.
//!
//! The installed entrypoint is `homma hook run <event>` with the clone as the working directory.
//! Refusing it over a key the commit and push gates have no business with would lock every
//! attached non-member clone out of committing.

use std::path::{Path, PathBuf};

use predicates::prelude::*;

mod support;
use support::{bin, git};

const MANIFEST: &str = "[workspace]\nname = \"w\"\npath = \".\"\n";

/// A workspace with two clones, `a` and `b`, and a local file scoping the work to `a` and giving
/// every clone one `post-merge` entry.
fn workspace() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    for name in ["a", "b"] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            git()
                .args(["init", "-q", "-b", "main"])
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(root.join("homma.toml"), MANIFEST).unwrap();
    std::fs::write(
        root.join("homma.local.toml"),
        "[instance]\nwork = \"x\"\nrepos = [\"a\"]\n[[hooks.post-merge]]\nrun = \"echo GATE-RAN\"\n",
    )
    .unwrap();
    (d, root)
}

fn homma(root: &Path) -> assert_cmd::Command {
    let mut c = bin();
    c.arg("--dir").arg(root);
    c
}

#[test]
fn a_hook_runs_in_a_clone_outside_the_scope() {
    let (_d, root) = workspace();
    homma(&root)
        .current_dir(root.join("b"))
        .args(["hook", "run", "post-merge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("GATE-RAN"));
}

#[test]
fn a_hook_runs_in_a_clone_inside_the_scope_too() {
    // the control: the same call from the scoped clone, so the test above is about scope
    let (_d, root) = workspace();
    homma(&root)
        .current_dir(root.join("a"))
        .args(["hook", "run", "post-merge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("GATE-RAN"));
}

#[test]
fn the_workspace_repository_itself_still_refuses_a_hook_run() {
    // the fallback is for a clone outside the scope and for nothing else
    let (_d, root) = workspace();
    assert!(
        git()
            .args(["init", "-q", "-b", "main"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success()
    );
    homma(&root)
        .current_dir(&root)
        .args(["hook", "run", "post-merge"])
        .assert()
        .failure();
}

#[test]
fn naming_a_clone_outside_the_scope_says_it_is_outside_the_scope() {
    let (_d, root) = workspace();
    homma(&root)
        .args(["hook", "install", "b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside this clone's scope"));
}
