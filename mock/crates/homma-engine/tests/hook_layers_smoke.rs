//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The hooks table read from every layer, end to end: a hook declared only in
//! `homma.local.toml` or another named overlay is installed and runs, a file
//! nobody named is reported and never read, and a layer that cannot be read
//! stops the verbs that would otherwise run without it.

use std::path::{Path, PathBuf};

use predicates::prelude::*;

mod support;
use support::{bin, git};

const MANIFEST: &str = "[workspace]\nname = \"w\"\npath = \".\"\n";

/// A workspace with one member repository, `r`, and the manifest written.
fn workspace(manifest: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    let member = root.join("r");
    std::fs::create_dir_all(&member).unwrap();
    assert!(
        git()
            .args(["init", "-q", "-b", "main"])
            .current_dir(&member)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(root.join("homma.toml"), manifest).unwrap();
    (d, root, member)
}

fn homma(root: &Path) -> assert_cmd::Command {
    let mut c = bin();
    c.arg("--dir").arg(root);
    c
}

#[test]
fn a_hook_declared_only_in_the_clones_own_file_is_installed_and_runs() {
    let (_d, root, member) = workspace(MANIFEST);
    std::fs::write(
        root.join("homma.local.toml"),
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"echo LAYER-RAN\"\n",
    )
    .unwrap();

    homma(&root)
        .args(["hook", "install", "r"])
        .assert()
        .success();
    assert!(
        member.join(".git/hooks/post-merge").is_file(),
        "an event only the layer names has its entrypoint"
    );
    // and with the layer gone the event has none, which is the control
    std::fs::remove_file(root.join("homma.local.toml")).unwrap();
    std::fs::remove_file(member.join(".git/hooks/post-merge")).unwrap();
    homma(&root)
        .args(["hook", "install", "r"])
        .assert()
        .success();
    assert!(!member.join(".git/hooks/post-merge").exists());

    // the entry runs from the layer
    std::fs::write(
        root.join("homma.local.toml"),
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"echo LAYER-RAN\"\n",
    )
    .unwrap();
    homma(&root)
        .current_dir(&member)
        .args(["hook", "run", "post-merge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("LAYER-RAN"));
}

#[test]
fn a_named_overlay_is_read_and_one_nobody_named_is_reported_and_not() {
    let (_d, root, member) = workspace(&format!("layers = [\"homma.team.toml\"]\n{MANIFEST}"));
    std::fs::write(
        root.join("homma.team.toml"),
        "[[hooks.post-merge]]\nrun = \"echo TEAM-RAN\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("homma.stray.toml"),
        "[[hooks.post-merge]]\nrun = \"echo STRAY-RAN\"\n",
    )
    .unwrap();

    let out = homma(&root)
        .current_dir(&member)
        .args(["hook", "run", "post-merge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TEAM-RAN"))
        .stdout(predicate::str::contains("STRAY-RAN").not());
    drop(out);

    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("read      homma.team.toml"))
        .stdout(predicate::str::contains("stray     homma.stray.toml"));
    std::fs::remove_file(root.join("homma.stray.toml")).unwrap();
    homma(&root).args(["hook", "doctor"]).assert().success();
}

#[test]
fn a_layer_that_cannot_be_read_stops_the_verbs_and_not_the_rest_of_homma() {
    let (_d, root, member) = workspace(MANIFEST);
    std::fs::write(
        root.join("homma.local.toml"),
        "[[hooks.post-merge]]\nrun = \n",
    )
    .unwrap();

    for args in [vec!["hook", "install", "r"], vec!["hook", "run", "post-merge"]] {
        homma(&root)
            .current_dir(&member)
            .args(&args)
            .assert()
            .failure()
            .stdout(predicate::str::contains("homma.local.toml"));
    }
    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error     "));
    // the manifest still loads, so `homma local` gets as far as its own reader,
    // which names the broken file itself; a refused manifest load would blame
    // homma.toml instead
    homma(&root).args(["local", "show"]).assert().stderr(
        predicate::str::contains("homma.local.toml")
            .and(predicate::str::contains("homma.toml does not parse").not()),
    );
}
