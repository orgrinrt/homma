//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which clones a release verb can find: every clone of the workspace for a verb that acts on one
//! repository, and only the scope for the sweep that names none.

use clap::Parser;

use super::*;
use crate::cli::Command;

/// Two clones under a workspace whose local file names only `inside`.
fn two_clones_and_a_scope_of_one() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    std::fs::write(
        dir.path().join(homma_core::local::LOCAL_FILE),
        "[instance]\nwork = \"w\"\nrepos = [\"inside\"]\n",
    )
    .unwrap();
    for name in ["inside", "outside"] {
        std::fs::create_dir_all(dir.path().join(name).join(".git")).unwrap();
    }
    dir
}

/// The repositories the config a release command line loads holds.
fn found_by(dir: &Path, args: &[&str]) -> Vec<String> {
    let mut argv = vec!["homma-engine", "--dir", dir.to_str().unwrap(), "release"];
    argv.extend_from_slice(args);
    let cli = Cli::try_parse_from(argv).expect("these arguments should parse");
    let Command::Release {
        op,
    } = &cli.command
    else {
        panic!("a release command line parsed as something else");
    };
    config_for(&cli, op)
        .unwrap()
        .repos
        .keys()
        .cloned()
        .collect()
}

#[test]
fn a_verb_that_names_a_repository_finds_it_outside_the_scope() {
    let dir = two_clones_and_a_scope_of_one();
    for args in [
        &["check", "outside"][..],
        &["plan", "outside", "--level", "patch"],
        &["run", "outside", "--level", "patch"],
        &["run", "outside", "--level", "patch", "--dry-run"],
        &["gate", "outside"],
        &["badges", "outside"],
    ] {
        let found = found_by(dir.path(), args);
        assert_eq!(found, ["inside", "outside"], "{args:?}");
    }
}

/// A verb that leaves the repository to the working directory finds a clone outside the scope too.
#[test]
fn a_verb_that_names_none_but_a_directory_finds_every_clone() {
    let dir = two_clones_and_a_scope_of_one();
    assert_eq!(found_by(dir.path(), &["check"]), ["inside", "outside"]);
    assert_eq!(found_by(dir.path(), &["plan", "--level", "patch"]), [
        "inside", "outside"
    ]);
}

/// The control: the sweep keeps the scope, since a container attaches clones it only reads and a sweep over
/// every clone would try to release those. Without it the test above is equally consistent with a command
/// that ignores the scope everywhere.
#[test]
fn the_sweep_that_names_no_repository_stays_in_the_scope() {
    let dir = two_clones_and_a_scope_of_one();
    assert_eq!(found_by(dir.path(), &["run", "--level", "patch"]), [
        "inside"
    ]);
    assert_eq!(
        found_by(dir.path(), &["run", "--level", "patch", "--dry-run"]),
        ["inside"]
    );
}
