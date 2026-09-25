//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The shared `settings.json` against the local file: that a local file which
//! refuses leaves the shared one as it was, and that the shared one is swept
//! of homma's registrations whichever repo they name.

use super::*;

/// A shared file holding one of homma's registrations, so a sweep of it is
/// due and would rewrite it.
const SHARED_WITH_ONE_OF_HOMMAS: &str = r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--old.sh"}]}]}}"#;

/// Every way the local file can refuse, each of which used to arrive after the
/// shared file had already been swept and written.
#[test]
fn a_local_file_that_refuses_leaves_the_shared_one_as_it_was() {
    for local in ["{ not json", "[]", r#"{"hooks":[]}"#, r#"{"hooks":{"PreToolUse":{}}}"#] {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        seed_shared(workspace, SHARED_WITH_ONE_OF_HOMMAS);
        seed(workspace, local);

        let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
        let r = merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None);

        assert!(r.is_err(), "a local file of {local} is refused");
        assert_eq!(
            fs::read_to_string(workspace.join(SHARED)).unwrap(),
            SHARED_WITH_ONE_OF_HOMMAS,
            "the shared file is untouched when the local file is {local}"
        );
        assert_eq!(fs::read_to_string(workspace.join(LOCAL)).unwrap(), local);
    }
}

#[test]
fn a_local_file_that_parses_lets_the_same_shared_file_be_swept() {
    // The control for the arm above: the same shared file, a local file that
    // is fine, and the sweep happens.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(workspace, SHARED_WITH_ONE_OF_HOMMAS);
    seed(workspace, "{}");

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    assert!(commands(&shared).is_empty(), "{shared}");
}

#[test]
fn the_shared_file_is_swept_of_every_repos_registrations_visited_or_not() {
    // `kamu` is declared and not visited, `muisti` is not even declared, which
    // is a clone that never held it; the shared file holds both anyway, and the
    // gate and a declared hook beside them.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/kamu--gate.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/muisti--no-yagni-guard.sh"}]},
            {"matcher":"*","hooks":[{"type":"command","command":".claude/hooks/_workspace--mockspace-gate.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/_declared--PreToolUse--_shared_scripts_guard.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
        ]}}"#,
    );

    merge_settings(
        &test_root(workspace),
        &["arvo", "kamu"],
        &["arvo"],
        &[],
        None,
    )
    .unwrap();

    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    assert_eq!(commands(&shared), vec![".claude/hooks/mine.sh"]);
}

#[test]
fn an_unvisited_repos_registration_is_kept_in_the_local_file_and_swept_from_the_shared_one() {
    // One command, one run, both files: the two predicates side by side.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let body = r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/kamu--gate.sh"}]}]}}"#;
    seed_shared(workspace, body);
    seed(workspace, body);

    merge_settings(
        &test_root(workspace),
        &["arvo", "kamu"],
        &["arvo"],
        &[],
        None,
    )
    .unwrap();

    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    assert!(commands(&shared).is_empty(), "{shared}");
    assert_eq!(commands(&read(workspace)), vec![
        ".claude/hooks/kamu--gate.sh"
    ]);
}

#[test]
fn a_hand_written_name_in_hommas_shape_with_no_file_is_swept_as_homma_s() {
    // The design gives the `<repo>--<rest>` name under `.claude/hooks/` to
    // homma, so a hook named that way with no file here is one of homma's
    // left behind, whoever wrote it; the arm below is the one thing that keeps
    // such a name.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/pre--commit.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":"scripts/pre--commit.sh"}]}
        ]}}"#,
    );

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &[], None).unwrap();

    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    assert_eq!(commands(&shared), vec!["scripts/pre--commit.sh"]);
}

#[test]
fn a_file_under_hommas_shape_that_homma_did_not_write_keeps_its_shared_registration() {
    // Somebody's own hook, named the way homma names one, and two names that
    // only look like the shape: nothing before the separator, nothing after.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    fs::create_dir_all(workspace.join(".claude/hooks")).unwrap();
    fs::write(
        workspace.join(".claude/hooks/muisti--mine.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    let body = r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/muisti--mine.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/--odd.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/odd--"}]}
        ]}}"#;
    seed_shared(workspace, body);

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &[], None).unwrap();

    // Nothing in it was homma's, so it was not even rewritten.
    assert_eq!(fs::read_to_string(workspace.join(SHARED)).unwrap(), body);
}
