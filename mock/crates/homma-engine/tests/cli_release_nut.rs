//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! A nutshell package, `nut.toml` declared `nut` under `[markers]`, is
//! released off the version its `[package]` carries, through the binary and
//! off the network.

mod support;

use predicates::prelude::*;
use support::{bin, clone_at, git_in, minimal_config_toml};

fn committed_nut(root: &std::path::Path, name: &str) {
    clone_at(root, name, Some("https://github.com/orgrinrt/x.git"));
    let path = root.join(name);
    std::fs::write(
        path.join("nut.toml"),
        "nutshell_branch = \"dev\"\n\n[package]\nname = \"x\"\nversion = \"0.2.0\"\n\n[meta]\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    for args in [
        &["config", "user.email", "t@t"][..],
        &["config", "user.name", "t"],
        &["config", "commit.gpgsign", "false"],
        &["add", "."],
        &["commit", "-qm", "feat: first"],
    ] {
        git_in(&path, args);
    }
}

#[test]
fn a_nut_package_plans_its_next_version_from_nut_toml() {
    // Mutation: `nut` read as content, which carries no version, ends the
    // plan with `manifest: no manifest`.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "{}\n[markers]\n\"nut.toml\" = \"nut\"\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    committed_nut(dir.path(), "x");
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "minor"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "version 0.2.0 becomes 0.3.0, tagged `v0.3.0`",
        ))
        // nothing goes to a registry: a nutshell library is imported by its
        // git uri at the tag
        .stdout(predicate::str::contains("crates-io").not())
        .stdout(predicate::str::contains("jsr").not());
    // a plan moves nothing
    let text = std::fs::read_to_string(dir.path().join("x/nut.toml")).unwrap();
    assert!(text.contains("version = \"0.2.0\""), "{text}");
}

#[test]
fn a_nut_toml_declared_content_carries_no_version() {
    // The workspace's row before it names `nut`: still refused, as before.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "{}\n[markers]\n\"nut.toml\" = \"content\"\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    committed_nut(dir.path(), "x");
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "minor"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no manifest"));
}
