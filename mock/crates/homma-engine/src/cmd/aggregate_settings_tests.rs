//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The merge into the clone's `settings.local.json` and the sweep of the
//! shared `settings.json`: what each takes, what each keeps.

use std::fs;

use super::*;
use crate::cmd::aggregate::HookEntry;
use crate::cmd::aggregate::tests::test_root;

/// The clone's `settings.local.json` after the merge, parsed.
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
fn the_shared_file_is_not_written_when_nothing_in_it_is_hommas() {
    // Formatting nobody's serialiser produces, so a rewrite of the same
    // content would still show.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let body = "{ \"env\": {\"A\": \"1\"},\n  \"hooks\": {\"PreToolUse\": [{\"matcher\": \"Bash\", \"hooks\": [{\"type\": \"command\", \"command\": \".claude/hooks/mine.sh\"}]}]} }";
    seed_shared(workspace, body);

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    assert_eq!(fs::read_to_string(workspace.join(SHARED)).unwrap(), body);
    assert_eq!(commands(&read(workspace)), vec![
        ".claude/hooks/arvo--new.sh"
    ]);
}

#[test]
fn a_shared_file_with_no_hooks_is_not_given_any() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let body = "{\"env\":{\"A\":\"1\"}}";
    seed_shared(workspace, body);

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    assert_eq!(fs::read_to_string(workspace.join(SHARED)).unwrap(), body);
}

#[test]
fn an_absent_shared_file_stays_absent() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    assert!(!workspace.join(SHARED).exists());
    assert_eq!(commands(&read(workspace)), vec![
        ".claude/hooks/arvo--new.sh"
    ]);
}

#[test]
fn hommas_registrations_in_the_shared_file_are_swept_and_everything_else_kept() {
    // What an earlier regen left in the shared file, beside what somebody
    // wrote there by hand and a key that is not about hooks at all.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(
        workspace,
        r#"{"env":{"A":"1"},"hooks":{
            "PreToolUse":[
                {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--old.sh"}]},
                {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
            ],
            "Stop":[{"hooks":[{"type":"command","command":".claude/hooks/arvo--stop.sh"}]}]
        }}"#,
    );

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    let shared: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(workspace.join(SHARED)).unwrap()).unwrap();
    assert_eq!(commands(&shared), vec![".claude/hooks/mine.sh"]);
    assert_eq!(shared["env"]["A"], "1");
    assert!(
        shared["hooks"].get("Stop").is_none(),
        "an event the sweep emptied is dropped: {shared}"
    );
    // The new registration went to the clone's file and not to the shared one.
    assert_eq!(commands(&read(workspace)), vec![
        ".claude/hooks/arvo--new.sh"
    ]);
}

#[test]
fn the_local_files_other_keys_are_kept() {
    // The clone's file also holds what the person allowed and prefers.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"permissions":{"allow":["Artifact"]},"outputStyle":"Concise"}"#,
    );

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();

    let v = read(workspace);
    assert_eq!(v["permissions"]["allow"][0], "Artifact");
    assert_eq!(v["outputStyle"], "Concise");
    assert_eq!(commands(&v), vec![".claude/hooks/arvo--new.sh"]);
}

#[test]
fn a_second_regen_writes_the_same_local_file() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(workspace, r#"{"outputStyle":"Concise"}"#);
    let entries = vec![
        entry("Edit", ".claude/hooks/arvo--a.sh"),
        entry("Write", ".claude/hooks/arvo--b.sh"),
    ];

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
    let first = fs::read_to_string(workspace.join(LOCAL)).unwrap();
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
    let second = fs::read_to_string(workspace.join(LOCAL)).unwrap();

    assert_eq!(first, second);
    assert_eq!(commands(&read(workspace)).len(), 2);
}

#[test]
fn a_shared_file_that_does_not_parse_is_an_error_and_the_local_one_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed_shared(workspace, "{ not json");

    let entries = vec![entry("Edit", ".claude/hooks/arvo--new.sh")];
    let r = merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None);

    assert!(r.is_err());
    assert!(!workspace.join(LOCAL).exists());
    assert_eq!(
        fs::read_to_string(workspace.join(SHARED)).unwrap(),
        "{ not json"
    );
}

#[path = "aggregate_settings_shared_tests.rs"]
mod shared;

fn entry(matcher: &str, command: &str) -> HookEntry {
    HookEntry {
        event:   "PreToolUse".into(),
        matcher: matcher.into(),
        command: command.into(),
    }
}

#[test]
fn merge_settings_preserves_hand_authored_entries() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}]}}"#,
    );

    let entries = vec![entry("Edit", ".claude/hooks/arvo--no-alloc.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
    let v = read(workspace);
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(
        arr.iter()
            .any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh")
    );
    assert!(
        arr.iter()
            .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--no-alloc.sh")
    );
}

#[test]
fn merge_settings_replaces_previously_aggregated() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--old.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}
        ]}}"#,
    );

    let entries = vec![entry("Write", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
    let v = read(workspace);
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(
        !arr.iter()
            .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--old.sh")
    );
    assert!(
        arr.iter()
            .any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh")
    );
    assert!(
        arr.iter()
            .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--new.sh")
    );
}

#[test]
fn merge_settings_preserves_mixed_hook_entries() {
    // An entry with one aggregated hook AND one hand-authored hook
    // in the same `hooks[]` array. Per-hook filtering must strip
    // only the aggregated hook and keep the entry intact with the
    // hand-authored hook surviving.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[
                {"type":"command","command":".claude/hooks/arvo--old.sh"},
                {"type":"command","command":".claude/hooks/workspace-handauthored.sh"}
            ]}
        ]}}"#,
    );

    let entries = vec![entry("Write", ".claude/hooks/arvo--new.sh")];
    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
    let v = read(workspace);
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    // Original Edit entry preserved, with `arvo--old.sh` stripped
    // and `workspace-handauthored.sh` surviving. Plus the freshly
    // pushed `arvo--new.sh`.
    assert_eq!(arr.len(), 2);
    let edit = arr.iter().find(|e| e["matcher"] == "Edit").unwrap();
    let edit_hooks = edit["hooks"].as_array().unwrap();
    assert_eq!(
        edit_hooks.len(),
        1,
        "aggregated hook should be stripped, hand-authored preserved"
    );
    assert_eq!(
        edit_hooks[0]["command"],
        ".claude/hooks/workspace-handauthored.sh"
    );
    let write = arr.iter().find(|e| e["matcher"] == "Write").unwrap();
    assert_eq!(write["hooks"][0]["command"], ".claude/hooks/arvo--new.sh");
}

#[test]
fn merge_settings_keeps_entries_for_a_known_repo_this_run_did_not_visit() {
    // A workspace clones the repos its work touches, so most of the
    // manifest aggregates nothing on any given run. Sweeping on the full
    // manifest deleted those registrations, and the wrapper files survived
    // because the cleanup runs inside the per-repo pass that was skipped.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/kolli--guard.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"/elsewhere/.claude/hooks/kolli--stale.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--old.sh"}]}
        ]}}"#,
    );

    let entries = vec![entry(
        "Edit",
        "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--new.sh",
    )];
    merge_settings(
        &test_root(workspace),
        &["arvo", "kolli"],
        &["arvo"],
        &entries,
        None,
    )
    .unwrap();

    let v = read(workspace);
    let cmds: Vec<String> = v["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .map(|h| h["command"].as_str().unwrap().to_string())
        .collect();

    assert!(
        cmds.iter().any(|c| c.ends_with("kolli--guard.sh")),
        "an unvisited repo's registration was swept: {cmds:?}"
    );
    // The control on the same run: the repo that WAS visited is rewritten,
    // so preservation is not the whole predicate.
    assert!(
        !cmds.iter().any(|c| c.ends_with("arvo--old.sh")),
        "a visited repo's stale registration survived: {cmds:?}"
    );
    assert!(cmds.iter().any(|c| c.ends_with("arvo--new.sh")));
    // And the unvisited repo's *absolute* entry goes, because no clone can
    // resolve it. Preservation is about the placeholder form only.
    assert!(
        !cmds.iter().any(|c| c.ends_with("kolli--stale.sh")),
        "a retired absolute entry survived on an unvisited repo: {cmds:?}"
    );
}

#[test]
fn merge_settings_sweeps_the_legacy_shape_for_any_known_repo_visited_or_not() {
    // The retired bash aggregator's `imports/<repo>/` form. Nothing writes
    // it any more, so there is no workspace where keeping it makes it work,
    // and it is swept on the full manifest rather than on the visited set.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/imports/kolli/guard.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
        ]}}"#,
    );

    merge_settings(
        &test_root(workspace),
        &["arvo", "kolli"],
        &["arvo"],
        &[],
        None,
    )
    .unwrap();

    let body = fs::read_to_string(workspace.join(LOCAL)).unwrap();
    assert!(
        !body.contains("imports/kolli"),
        "legacy entry survived: {body}"
    );
    // The control: a hand-authored entry is not swept alongside it.
    assert!(
        body.contains("mine.sh"),
        "hand-authored entry was swept: {body}"
    );
}

#[test]
fn merge_settings_drops_entry_when_all_hooks_aggregated() {
    // Inverse of the mixed-hook test: when every hook in an entry
    // is aggregated, the entry collapses to empty and gets dropped.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[
                {"type":"command","command":".claude/hooks/arvo--old1.sh"},
                {"type":"command","command":".claude/hooks/arvo--old2.sh"}
            ]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}
        ]}}"#,
    );

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &[], None).unwrap();
    let v = read(workspace);
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    // Edit entry collapsed (all hooks were aggregated); Bash entry
    // survives.
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["matcher"], "Bash");
}

#[test]
fn a_registration_whose_managed_file_is_gone_is_swept_and_one_whose_file_is_somebodys_is_kept() {
    // The design's sweep of a registration naming a file homma writes that is
    // no longer there, beside its control: the same name held by a file homma
    // did not write keeps its registration.
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    seed(
        workspace,
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--gone.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--theirs.sh"}]}
        ]}}"#,
    );
    fs::create_dir_all(workspace.join(".claude/hooks")).unwrap();
    fs::write(
        workspace.join(".claude/hooks/arvo--theirs.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &[], None).unwrap();
    let body = fs::read_to_string(workspace.join(LOCAL)).unwrap();
    assert!(!body.contains("arvo--gone.sh"), "{body}");
    assert!(body.contains("arvo--theirs.sh"), "{body}");
}
