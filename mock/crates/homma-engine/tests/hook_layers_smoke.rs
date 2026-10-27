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
fn doctor_with_a_repository_fails_until_the_entrypoints_are_installed() {
    let (_d, root, _member) = workspace(MANIFEST);
    std::fs::write(
        root.join("homma.local.toml"),
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"true\"\n",
    )
    .unwrap();

    // no hooks path is set, and nothing is installed: git runs nothing, and
    // the report must not say otherwise
    homma(&root)
        .args(["hook", "doctor", "r"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("post-merge"))
        .stdout(predicate::str::contains("homma hook install"));

    // and the control: once installed it passes
    homma(&root)
        .args(["hook", "install", "r"])
        .assert()
        .success();
    homma(&root)
        .args(["hook", "doctor", "r"])
        .assert()
        .success();
}

#[test]
fn a_broken_layer_is_listed_once_with_the_readers_reason() {
    let (_d, root, _member) = workspace(MANIFEST);
    std::fs::write(
        root.join("homma.local.toml"),
        "[[hooks.post-merge]]\nrun = \n",
    )
    .unwrap();

    let out = homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("error     ")).count(),
        1,
        "one error line: {text}"
    );
    assert!(text.contains("line 2"), "{text}");
    assert!(!text.contains("read      homma.local.toml"), "{text}");
}

#[test]
fn the_clones_own_file_is_a_note_and_not_a_failure_when_layers_does_not_name_it() {
    let (_d, root, _member) = workspace(&format!("layers = [\"homma.team.toml\"]\n{MANIFEST}"));
    std::fs::write(root.join("homma.local.toml"), "[instance]\nwork = \"x\"\n").unwrap();

    let out = homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("note      homma.local.toml is not named in `layers`"),
        "{text}"
    );
    assert!(!text.contains("stray"), "{text}");
    // a file that is nobody's still fails, which is the control
    std::fs::write(root.join("homma.other.toml"), "").unwrap();
    homma(&root).args(["hook", "doctor"]).assert().failure();
}

#[test]
fn a_parse_cause_quoting_another_layer_does_not_hide_that_layers_read_line() {
    let (_d, root, _member) = workspace(&format!(
        "layers = [\"homma.a.toml\", \"homma.b.toml\"]\n{MANIFEST}"
    ));
    // the offending line is quoted in the reader's reason, and it names b
    std::fs::write(
        root.join("homma.a.toml"),
        "[[hooks.post-merge]]\nrun = homma.b.toml\n",
    )
    .unwrap();
    std::fs::write(
        root.join("homma.b.toml"),
        "[[hooks.post-merge]]\nrun = \"b\"\n",
    )
    .unwrap();

    let out = homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("2 | run = homma.b.toml"),
        "control: the cause really quotes the other layer: {text}"
    );
    assert!(text.contains("read      homma.b.toml"), "{text}");
    assert!(!text.contains("read      homma.a.toml"), "{text}");
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
    // the agent aggregate writes back only the rows it has, so it refuses
    // naming the file rather than regenerating without the layer's guards
    homma(&root)
        .args(["agent", "regen", "--skip-cargo-mock", "--skip-configs"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("homma.local.toml"));
    // and the control: with the aggregate skipped nothing needs the table, so
    // the refusal is specific to the stage that would lose rows
    homma(&root)
        .args(["agent", "regen", "--skip-cargo-mock", "--skip-aggregate"])
        .assert()
        .stderr(predicate::str::contains("incomplete").not());
    // the manifest still loads, so `homma local` gets as far as its own reader,
    // which names the broken file itself; a refused manifest load would blame
    // homma.toml instead
    homma(&root).args(["local", "show"]).assert().stderr(
        predicate::str::contains("homma.local.toml")
            .and(predicate::str::contains("homma.toml does not parse").not()),
    );
}
