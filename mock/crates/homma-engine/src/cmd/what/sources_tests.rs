//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::doc::Kind;
use super::fixture::workspace;
use super::sources::{corpora, scripts, verbs};

/// Failing control: with the planted directory kept in a static, its root outlives the test.
#[test]
fn a_planted_workspace_is_gone_once_its_holder_drops_it() {
    let ws = workspace();
    let root = ws.path().to_path_buf();
    assert!(root.join(".shared/scripts/chores/harvest").is_file());
    drop(ws);
    assert!(!root.exists(), "{} is left behind", root.display());
}

#[test]
fn a_planted_scripts_verbs_and_place_are_read() {
    let g = scripts::gather(workspace().path());
    let s = g
        .docs
        .iter()
        .find(|d| d.name == "harvest")
        .expect("the harvest script is read");
    for v in ["ripe", "rotten"] {
        assert!(
            s.verbs.iter().any(|x| x.name == v),
            "no verb {v}: {:?}",
            s.verbs
        );
    }
    assert_eq!(s.place, ["chores"]);
}

#[test]
fn tests_and_fixtures_are_not_scripts() {
    let g = scripts::gather(workspace().path());
    assert!(g.docs.iter().any(|d| d.name == "water-plants"));
    assert!(
        g.docs
            .iter()
            .all(|d| !d.name.contains("_test") && !d.name.contains("_fixture")),
        "a test is listed"
    );
    assert!(g.docs.iter().all(|d| d.name != "test"));
}

fn plant(root: &std::path::Path, rel: &str, text: &str, exec: bool) {
    let mode = if exec { 0o755 } else { 0o644 };
    crate::cmd::planted::script(&root.join(".shared/scripts").join(rel), text, mode);
}

#[test]
fn the_scripts_without_a_header_are_counted_in_the_footer() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    plant(
        dir.path(),
        "a/two.py",
        "#!/usr/bin/env python3\n\"\"\"two: does two.\"\"\"\n",
        false,
    );
    plant(dir.path(), "a/bare", "#!/bin/sh\necho hi\n", true);
    plant(dir.path(), "b/bare2.sh", "#!/bin/sh\necho hi\n", false);
    plant(dir.path(), "b/notes.txt", "not a script\n", false);
    plant(dir.path(), "b/one_test.sh", "echo test\n", true);
    let g = scripts::gather(dir.path());
    let mut names: Vec<&str> = g.docs.iter().map(|d| d.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["one", "two"],
        "a non-executable script with an extension is read"
    );
    assert_eq!(g.gaps.len(), 1);
    assert_eq!(
        g.gaps[0].count, 2,
        "the bare executable and the bare shebang script are counted, the test and the text are not"
    );
    assert_eq!(
        g.gaps[0].what,
        "2 scripts under .shared/scripts carry no header comment, so they cannot be found by what they do"
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_a_directory_is_not_followed_so_it_cannot_loop() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    let a = dir.path().join(".shared/scripts/a");
    std::os::unix::fs::symlink("..", a.join("up")).unwrap();
    std::os::unix::fs::symlink("..", a.join("up2")).unwrap();
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.len(),
        1,
        "{:?}",
        g.docs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_workspace_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    plant(
        outside.path(),
        "x/secret",
        "#!/bin/sh\n# secret - outside\n",
        true,
    );
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    let a = dir.path().join(".shared/scripts/a");
    std::os::unix::fs::symlink(outside.path().join(".shared/scripts/x"), a.join("ext")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join(".shared/scripts/x/secret"),
        a.join("linked"),
    )
    .unwrap();
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}

#[test]
fn no_scripts_directory_is_a_gap_and_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let g = scripts::gather(dir.path());
    assert!(g.docs.is_empty());
    assert_eq!(
        g.gaps[0].what,
        "no .shared/scripts here: scripts not searched"
    );
    assert!(
        corpora::skills(dir.path()).gaps[0]
            .what
            .contains("skills not searched")
    );
    assert!(
        corpora::rules(dir.path()).gaps[0]
            .what
            .contains("rules not searched")
    );
}

#[test]
fn homma_verbs_are_read_from_the_command_line_itself() {
    let g = verbs::gather();
    let rules = g
        .docs
        .iter()
        .find(|d| d.name == "rules")
        .expect("the rules command");
    assert_eq!(rules.kind, Kind::Verb);
    assert!(
        rules
            .verbs
            .iter()
            .any(|v| v.name == "about" && v.command == "homma rules about"),
        "{:?}",
        rules.verbs
    );
    assert!(
        g.docs.iter().any(|d| d.name == "what"),
        "this command finds itself"
    );
    assert!(g.docs.iter().all(|d| d.name != "help"));
}
#[test]
fn the_planted_skills_and_rules_are_read() {
    let ws = workspace();
    let s = corpora::skills(ws.path());
    assert!(
        s.docs
            .iter()
            .any(|d| d.name == "plan-a-season" && !d.summary.is_empty())
    );
    let r = corpora::rules(ws.path());
    assert!(
        r.docs
            .iter()
            .any(|d| d.command == format!("homma rules show {}", d.name) && !d.tags.is_empty())
    );
}

#[cfg(unix)]
#[test]
fn the_planted_tools_carry_their_verbs_and_how_to_run_them() {
    let g = super::sources::tools::gather(workspace().path());
    let t = g
        .docs
        .iter()
        .find(|d| d.name == "plot-weed")
        .expect("plot-weed");
    assert_eq!(
        t.command, "homma plot-weed <pull|tally>",
        "bare, it names the verbs it wants"
    );
    assert!(
        t.verbs
            .iter()
            .any(|v| v.name == "pull" && v.command == "homma plot-weed pull")
    );
    assert!(t.tags.iter().any(|x| x == "garden"));
}

#[test]
fn a_programs_own_source_counts_only_where_executable_and_tests_never() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "p/src/run", "#!/bin/sh\n# run - runs\n", true);
    plant(
        dir.path(),
        "p/src/helper.sh",
        "#!/bin/sh\n# helper - helps\n",
        false,
    );
    plant(
        dir.path(),
        "p/lib/util.sh",
        "#!/bin/sh\n# util - utils\n",
        false,
    );
    plant(
        dir.path(),
        "p/tests/check",
        "#!/bin/sh\n# check - checks\n",
        true,
    );
    plant(
        dir.path(),
        "p/test_thing.sh",
        "#!/bin/sh\n# thing - tests\n",
        true,
    );
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["run"]
    );
}

#[cfg(unix)]
#[test]
fn the_scripts_directory_being_a_link_is_a_gap_and_not_a_way_out() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    plant(
        outside.path(),
        "x/secret",
        "#!/bin/sh\n# secret - outside\n",
        true,
    );
    std::fs::create_dir_all(dir.path().join(".shared")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join(".shared/scripts"),
        dir.path().join(".shared/scripts"),
    )
    .unwrap();
    let g = scripts::gather(dir.path());
    assert!(g.docs.is_empty(), "{:?}", g.docs);
    assert!(!g.gaps.is_empty());
}

#[cfg(unix)]
#[test]
fn an_executable_that_is_not_text_is_skipped_and_does_not_stop_the_walk() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    let bin = dir.path().join(".shared/scripts/a/blob");
    std::fs::write(&bin, [0xFF_u8, 0xFE, 0x00, 0x80]).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}

#[test]
fn a_local_config_that_does_not_parse_widens_the_scope_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("homma.local.toml"), "= = not toml [").unwrap();
    let (repos, gap) = super::scope(dir.path());
    assert_eq!(
        repos,
        homma_core::scope::Scope::everything(),
        "a config nobody could read narrows nothing"
    );
    let gap = gap.expect("the footer names it");
    assert!(
        gap.what.contains("homma.local.toml cannot be read"),
        "{}",
        gap.what
    );
}

#[test]
fn an_absent_tools_directory_is_a_gap_and_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let g = super::sources::tools::gather(dir.path());
    assert!(g.docs.is_empty());
    assert_eq!(g.gaps[0].what, "no tools/ here: tools not searched");
}

#[test]
fn a_catalogue_suite_is_not_a_script() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    plant(
        dir.path(),
        "a/guard_catalogue.sh",
        "#!/bin/sh\n# guard catalogue - cases kept red\n",
        true,
    );
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}

#[test]
fn a_file_another_script_sources_is_a_library_and_one_nobody_sources_is_not() {
    let dir = tempfile::tempdir().unwrap();
    plant(
        dir.path(),
        "m/main",
        "#!/bin/sh\n# main - the command\n# shellcheck source=lib-a.sh\n. \"$here/lib-a.sh\"\nsource \"$here/sub/lib-b.sh\"\n",
        true,
    );
    plant(
        dir.path(),
        "m/lib-a.sh",
        "#!/bin/sh\n# Functions the command uses.\nf() { :; }\n",
        false,
    );
    plant(
        dir.path(),
        "m/sub/lib-b.sh",
        "#!/bin/sh\n# More functions.\ng() { :; }\n",
        false,
    );
    plant(
        dir.path(),
        "m/lib-c.sh",
        "#!/bin/sh\n# lib-c - nobody sources this one\n",
        false,
    );
    plant(dir.path(), "n/lib.nut", "mod::one   one.sh\n", false);
    plant(
        dir.path(),
        "n/one.sh",
        "#!/bin/sh\n# A module.\nh() { :; }\n",
        false,
    );
    let g = scripts::gather(dir.path());
    let mut names: Vec<&str> = g.docs.iter().map(|d| d.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["lib-c", "main"]);
}

#[test]
fn the_directories_that_hold_no_tools_are_not_read() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    for skipped in ["probes", ".git", ".venv", "node_modules", "__pycache__"] {
        plant(
            dir.path(),
            &format!("a/{skipped}/hidden"),
            "#!/bin/sh\n# hidden - not a tool\n",
            true,
        );
    }
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}

#[test]
fn a_readme_row_that_begins_with_source_does_not_hide_a_script() {
    let dir = tempfile::tempdir().unwrap();
    plant(
        dir.path(),
        "w/README.md",
        "# w\n\nsource    parts.py     the parts\n",
        false,
    );
    plant(
        dir.path(),
        "w/parts.py",
        "#!/usr/bin/env python3\n\"\"\"parts - the parts\"\"\"\n",
        false,
    );
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["parts"]
    );
}

#[test]
fn a_header_that_says_sourced_makes_a_library_by_line_and_by_sentence() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    plant(
        dir.path(),
        "a/exec-lib",
        "#!/bin/sh\n# Sourced by nothing here.\n",
        true,
    );
    plant(
        dir.path(),
        "a/sentence-lib.sh",
        "#!/bin/sh\n# The helpers of the thing. Sourced by the thing.\n",
        false,
    );
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}
