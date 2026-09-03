//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

#![allow(dead_code, unused_imports)]
//! Shared test support.
//!
//! Re-exported rather than redefined. This helper existed in two crates, was
//! fixed in one, and the round that fixed it added a third copy. It now has one
//! definition, in `homma_core::testing`, and this module exists only so the
//! integration tests can write `support::` instead of naming the crate.

// The CLI fixtures, shared by `cli_smoke` and `cli_release`: the binary,
// the smallest manifest, and real clones beside it.
use std::path::{Path, PathBuf};

use assert_cmd::Command;
pub use homma_core::testing::global_configs_now;

pub fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

pub fn minimal_config_toml() -> String {
    r#"
[workspace]
name = "test-ws"
path = "."

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
"#
    .to_string()
}

/// A manifest beside one clone, which is the smallest workspace that has a
/// member at all.
pub fn write_tmp_config(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("homma.toml");
    std::fs::write(&path, minimal_config_toml()).unwrap();
    clone_at(
        dir.path(),
        "notko",
        Some("https://github.com/orgrinrt/notko.git"),
    );
    path
}

/// A real repository at `root/name`, with `origin` set to `url` when one is
/// given.
///
/// A real `git init` rather than a hand-made `.git` directory, because the
/// origin is read through git and a fake would answer nothing.
pub fn clone_at(root: &Path, name: &str, url: Option<&str>) {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    run(&["init", "-q"]);
    if let Some(url) = url {
        run(&["remote", "add", "origin", url]);
    }
}

pub fn committed_crate(root: &Path, name: &str) {
    clone_at(root, name, Some("https://github.com/orgrinrt/x.git"));
    let path = root.join(name);
    std::fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for args in [
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "."],
        vec!["commit", "-qm", "feat: first"],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
}

pub fn git_in(root: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}
