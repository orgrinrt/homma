//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which clones a release verb can find: every clone of the workspace for a verb that acts on one
//! repository, and only the scope for the sweep that names none.

use clap::Parser;

use super::*;
use crate::cli::Command;
use crate::cmd::util::resolve_repo_from;

/// Two clones under a workspace whose local file names only `inside`.
fn two_clones_and_a_scope_of_one() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    homma_core::testing::two_clones_and_a_scope_of_one(dir.path());
    dir
}

/// The config a release command line loads.
fn config_of(dir: &Path, args: &[&str]) -> Config {
    let mut argv = vec!["homma-engine", "--dir", dir.to_str().unwrap(), "release"];
    argv.extend_from_slice(args);
    let cli = Cli::try_parse_from(argv).expect("these arguments should parse");
    let Command::Release {
        op,
    } = &cli.command
    else {
        panic!("a release command line parsed as something else");
    };
    config_for(&cli, op).unwrap()
}

/// The repositories the config a release command line loads holds.
fn found_by(dir: &Path, args: &[&str]) -> Vec<String> {
    config_of(dir, args).repos.keys().cloned().collect()
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

/// The lookup a verb makes after the load, which is where the refusal the ticket reported came from: a
/// clone outside the scope is found by name, and from a directory inside it with no name given.
#[test]
fn a_clone_outside_the_scope_resolves_by_name_and_from_inside_it() {
    let dir = two_clones_and_a_scope_of_one();
    let inside_outside = dir.path().join("outside").join("src");
    std::fs::create_dir_all(&inside_outside).unwrap();

    let named = config_of(dir.path(), &["check", "outside"]);
    let (name, _, root) = resolve_repo_from(&named, Some("outside"), dir.path()).unwrap();
    assert_eq!(
        (name, root.file_name().and_then(|n| n.to_str())),
        ("outside", Some("outside"))
    );

    let unnamed = config_of(dir.path(), &["check"]);
    let (name, _, root) = resolve_repo_from(&unnamed, None, &inside_outside).unwrap();
    assert_eq!(
        (name, root.file_name().and_then(|n| n.to_str())),
        ("outside", Some("outside"))
    );
}

/// The control for the lookup: under the scoped load the same two asks are refused, the first as outside the
/// scope and the second as a directory that is in no repository, so the test above passes by the load and not
/// by a lookup that finds whatever is on disk.
#[test]
fn the_sweeps_config_refuses_the_same_lookups() {
    let dir = two_clones_and_a_scope_of_one();
    let inside_outside = dir.path().join("outside").join("src");
    std::fs::create_dir_all(&inside_outside).unwrap();
    let scoped = config_of(dir.path(), &["run", "--level", "patch"]);

    let by_name = resolve_repo_from(&scoped, Some("outside"), dir.path()).unwrap_err();
    assert!(
        by_name
            .to_string()
            .contains("is outside this clone's scope"),
        "{by_name}"
    );
    let from_inside = resolve_repo_from(&scoped, None, &inside_outside).unwrap_err();
    assert!(
        from_inside
            .to_string()
            .contains("not inside a workspace repository"),
        "{from_inside}"
    );
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
