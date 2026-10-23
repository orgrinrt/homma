//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What `hostile_git_identity_env` puts in a `git`'s environment.
//!
//! A fixture meant to make a test fail on every machine is only as hostile as the
//! keys it names. With one `GIT_CONFIG_COUNT` entry, a workspace defending five of
//! the six identity keys and not the sixth passed against it unseen.

use std::process::Command;

use homma_core::testing::hostile_git_identity_env;

/// The six keys `crates/homma-org/DESIGN.md.tmpl` lists for a provisioned clone.
const SIX_KEYS: [&str; 6] = [
    "user.name",
    "user.email",
    "author.name",
    "author.email",
    "committer.name",
    "committer.email",
];

/// What a `git` run under the fixture reads a configuration key as, with no file
/// in the way: the command-scope value, or nothing.
fn read(key: &str) -> Option<String> {
    let d = tempfile::tempdir().unwrap();
    let empty = d.path().join("empty.gitconfig");
    std::fs::write(&empty, "").unwrap();
    let mut git = Command::new("git");
    hostile_git_identity_env(&mut git);
    let out = git
        .args(["config", "--get", key])
        .current_dir(d.path())
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_CONFIG_SYSTEM", &empty)
        .output()
        .expect("git should run");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn every_one_of_the_six_identity_keys_is_set_hostile_in_the_environment() {
    for key in SIX_KEYS {
        let got = read(key);
        assert!(
            got.as_deref().is_some_and(|v| !v.is_empty()),
            "{key} is not set by the fixture, so a defence that skips it passes unseen"
        );
    }
}

#[test]
fn the_fixture_values_name_somebody_else() {
    for key in SIX_KEYS {
        let v = read(key).unwrap_or_default();
        assert!(
            v.contains("Env") || v.contains("env-") || v.ends_with("example.invalid"),
            "{key} is {v:?}, which is not recognisably hostile"
        );
    }
}
