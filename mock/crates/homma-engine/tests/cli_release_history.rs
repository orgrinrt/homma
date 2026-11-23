//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! `homma release run` reads the history it would release with the
//! workspace's `history_scan` before anything moves, through the binary and
//! off the network.

mod support;

use std::os::unix::fs::PermissionsExt;

use predicates::prelude::*;
use support::{bin, committed_crate, git_in, minimal_config_toml};

/// The manifest with `history_scan` naming a stand-in for the workspace's
/// scanner: exit 1 where the history carries a trailer naming an agent.
fn workspace_with_a_scan(dir: &std::path::Path) -> std::path::PathBuf {
    let scan = dir.join("scan");
    std::fs::write(
        &scan,
        "#!/bin/sh\nif git -C \"$1\" log --all --format=%B | grep -q '^Co-Authored-By: Claude'; then \
         echo \"trailer: Co-Authored-By: Claude\"; exit 1; fi; exit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&scan, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cfg = dir.join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "{}\n[paths]\nhistory_scan = \"scan\"\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    cfg
}

#[test]
fn a_release_of_a_flagged_history_is_refused_before_anything_moves() {
    // Mutation: the scan left out of the run, which tags and publishes the
    // trailer for good.
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_a_scan(dir.path());
    committed_crate(dir.path(), "x");
    let x = dir.path().join("x");
    std::fs::write(x.join("b"), "b\n").unwrap();
    git_in(&x, &["add", "b"]);
    git_in(&x, &[
        "commit",
        "-qm",
        "fix: b\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    ]);
    let head = std::process::Command::new("git")
        .args(["-C", x.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap()
        .stdout;
    bin()
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "release",
            "run",
            "x",
            "--level",
            "patch",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("carries attribution"))
        .stdout(predicate::str::contains("trailer: Co-Authored-By: Claude"))
        .stdout(predicate::str::contains("rehouse"));
    let after = std::process::Command::new("git")
        .args(["-C", x.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap()
        .stdout;
    assert_eq!(head, after, "nothing moved");
    assert!(!x.join("CHANGELOG.md").exists());
}
