//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The engine operates on the workspace it was pointed at, not on the one the
//! command happened to be typed in.
//!
//! This is the property the launcher depends on. It resolves the workspace root
//! itself, absolutely, and hands it over on `--dir`; if the engine then went on
//! reading `./homma.toml`, every launcher-run command would silently operate on
//! whatever directory the shell was sitting in.
//!
//! Every case here runs from a decoy workspace whose config differs from the
//! target's, so a run that ignored `--dir` succeeds and reports the wrong
//! answer rather than failing. The last case is the control: without the flag,
//! the decoy is what gets read, which is what makes the others mean something.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

/// A workspace whose name is its own, written into a fresh directory.
fn workspace_named(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("homma.toml"),
        format!(
            "content_repo = \"clause-dev\"\n\n[workspace]\nname = \"{name}\"\n\n\
             [org.op]\nrole = \"king\"\nhandle = \"op\"\n\n\
             [org.{name}]\nrole = \"hand\"\nhandle = \"{name}\"\ndomain = \"{name}\"\n"
        ),
    )
    .unwrap();
    dir
}

#[test]
fn status_reads_the_workspace_the_dir_flag_names_and_not_the_current_one() {
    let target = workspace_named("target");
    let decoy = workspace_named("decoy");

    bin()
        .current_dir(decoy.path())
        .args(["--dir", target.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("decoy").not());
}

#[test]
fn org_list_reads_it_too_rather_than_computing_its_own_path() {
    // The `org` arm carried its own copy of the config-path computation and so
    // would have gone on reading the current directory while every other
    // command honoured the flag. Two commands are named here because one
    // passing says nothing about the other when each resolves its own path.
    let target = workspace_named("target");
    let decoy = workspace_named("decoy");

    bin()
        .current_dir(decoy.path())
        .args(["--dir", target.path().to_str().unwrap(), "org", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("decoy").not());
}

#[test]
fn a_named_config_still_wins_over_the_directory() {
    let target = workspace_named("target");
    let named = workspace_named("named");

    bin()
        .current_dir(target.path())
        .args(["--dir", target.path().to_str().unwrap()])
        .args(["-c", named.path().join("homma.toml").to_str().unwrap()])
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("named"))
        .stdout(predicate::str::contains("target").not());
}

#[test]
fn without_the_flag_the_current_directory_is_what_gets_read() {
    // The control. If this reported anything other than the decoy, the three
    // cases above would pass whether or not `--dir` did anything at all.
    let decoy = workspace_named("decoy");

    bin()
        .current_dir(decoy.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("decoy"));
}
