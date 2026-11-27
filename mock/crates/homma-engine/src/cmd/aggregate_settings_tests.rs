//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The sweep of the registrations an older homma wrote, out of the clone's
//! `settings.local.json` and the shared `settings.json`: what it takes, what
//! it keeps, and that it writes nothing of its own.

use std::fs;

use super::*;
use crate::cmd::aggregate::tests::test_root;

/// The clone's `settings.local.json`, parsed.
fn read(workspace: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(workspace.join(LOCAL)).unwrap()).unwrap()
}

/// Seed the clone's `settings.local.json`.
fn seed(workspace: &std::path::Path, body: &str) {
    fs::create_dir_all(workspace.join(".claude")).unwrap();
    fs::write(workspace.join(LOCAL), body).unwrap();
}

/// Seed the shared `settings.json`.
fn seed_shared(workspace: &std::path::Path, body: &str) {
    fs::create_dir_all(workspace.join(".claude")).unwrap();
    fs::write(workspace.join(SHARED), body).unwrap();
}

/// Every command registered in a parsed settings file, under any event.
fn commands(v: &serde_json::Value) -> Vec<String> {
    v["hooks"]
        .as_object()
        .into_iter()
        .flat_map(|events| events.values())
        .flat_map(|entries| entries.as_array().cloned().unwrap_or_default())
        .flat_map(|e| e["hooks"].as_array().cloned().unwrap_or_default())
        .filter_map(|h| h["command"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn a_file_with_nothing_of_hommas_is_not_written() {
    // Formatting nobody's serialiser produces, so a rewrite of the same
    // content would still show.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let body = "{ \"env\": {\"A\": \"1\"},\n  \"hooks\": {\"PreToolUse\": [{\"matcher\": \"Bash\", \"hooks\": [{\"type\": \"command\", \"command\": \".claude/hooks/mine.sh\"}]}]} }";
    seed_shared(workspace, body);
    seed(workspace, body);
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    assert_eq!(fs::read_to_string(workspace.join(SHARED)).unwrap(), body);
    assert_eq!(fs::read_to_string(workspace.join(LOCAL)).unwrap(), body);
}

#[test]
fn absent_files_stay_absent() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    assert!(!workspace.join(SHARED).exists());
    assert!(!workspace.join(LOCAL).exists());
}

#[test]
fn hommas_registrations_are_swept_from_both_files_and_everything_else_kept() {
    // What an earlier regen left, beside what somebody wrote by hand, what
    // claude-shook registers, and a key that is not about hooks at all.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let body = r#"{"env":{"A":"1"},"permissions":{"allow":["Artifact"]},"hooks":{
        "PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/acre--old.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":"command -v claude-shook >/dev/null 2>&1 || { echo x >&2; exit 1; }; exec claude-shook run PreToolUse"}]}
        ],
        "Stop":[{"hooks":[{"type":"command","command":".claude/hooks/_declared--Stop--scripts_x.sh"}]}],
        "UserPromptSubmit":[{"hooks":[{"type":"command","command":".claude/hooks/_nudges--UserPromptSubmit.sh"}]}],
        "SessionStart":[{"hooks":[{"type":"command","command":".claude/hooks/_workspace--mockspace-gate.sh"}]}]
    }}"#;
    seed_shared(workspace, body);
    seed(workspace, body);
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    for v in [read(workspace), shared] {
        let cmds = commands(&v);
        assert_eq!(cmds.len(), 2, "{cmds:?}");
        assert_eq!(cmds[0], ".claude/hooks/mine.sh");
        assert!(cmds[1].contains("exec claude-shook run PreToolUse"));
        assert_eq!(v["env"]["A"], "1");
        assert_eq!(v["permissions"]["allow"][0], "Artifact");
        for gone in ["Stop", "UserPromptSubmit", "SessionStart"] {
            assert!(
                v["hooks"].get(gone).is_none(),
                "an event the sweep emptied is dropped: {v}"
            );
        }
    }
}

#[test]
fn a_member_this_clone_does_not_hold_is_swept_as_well() {
    // homma registers nothing of its own any more, so keeping an unvisited
    // member's registration would bring nothing back.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/krill--guard.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"/elsewhere/.claude/hooks/krill--stale.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/imports/krill/guard.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
        ]}}"#,
    );
    merge_settings(&test_root(workspace), &["acre", "krill"]).unwrap();
    assert_eq!(commands(&read(workspace)), vec![".claude/hooks/mine.sh"]);
}

#[test]
fn a_second_sweep_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"outputStyle":"Concise","hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/acre--a.sh"}]}]}}"#,
    );
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    let first = fs::read_to_string(workspace.join(LOCAL)).unwrap();
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    assert_eq!(fs::read_to_string(workspace.join(LOCAL)).unwrap(), first);
    assert_eq!(read(workspace)["outputStyle"], "Concise");
}

#[test]
fn a_shared_file_that_does_not_parse_is_an_error_and_the_local_one_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(workspace, "{ not json");
    let local = r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/acre--old.sh"}]}]}}"#;
    seed(workspace, local);
    assert!(merge_settings(&test_root(workspace), &["acre"]).is_err());
    assert_eq!(fs::read_to_string(workspace.join(LOCAL)).unwrap(), local);
    assert_eq!(
        fs::read_to_string(workspace.join(SHARED)).unwrap(),
        "{ not json"
    );
}

#[path = "aggregate_settings_shared_tests.rs"]
mod shared;

#[test]
fn a_hand_written_hook_beside_hommas_in_one_entry_survives_it() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[
                {"type":"command","command":".claude/hooks/acre--old.sh"},
                {"type":"command","command":".claude/hooks/workspace-handauthored.sh"}
            ]},
            {"matcher":"Write","hooks":[
                {"type":"command","command":".claude/hooks/acre--old1.sh"},
                {"type":"command","command":".claude/hooks/acre--old2.sh"}
            ]}
        ]}}"#,
    );
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    let v = read(workspace);
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    // the entry with a hand-written hook keeps it; the one the sweep emptied goes
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["matcher"], "Edit");
    assert_eq!(commands(&v), vec![
        ".claude/hooks/workspace-handauthored.sh"
    ]);
}

#[test]
fn a_registration_whose_managed_file_is_gone_is_swept_and_one_whose_file_is_somebodys_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/acre--gone.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/acre--theirs.sh"}]}
        ]}}"#,
    );
    fs::create_dir_all(workspace.join(".claude/hooks")).unwrap();
    fs::write(
        workspace.join(".claude/hooks/acre--theirs.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    merge_settings(&test_root(workspace), &["acre"]).unwrap();
    let body = fs::read_to_string(workspace.join(LOCAL)).unwrap();
    assert!(!body.contains("acre--gone.sh"), "{body}");
    assert!(body.contains("acre--theirs.sh"), "{body}");
}
