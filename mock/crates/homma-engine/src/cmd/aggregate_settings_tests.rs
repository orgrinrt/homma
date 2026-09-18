//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The merge into the workspace `settings.json`: what it sweeps, what it keeps.

use std::fs;

use super::*;
use crate::cmd::aggregate::HookEntry;
use crate::cmd::aggregate::tests::test_root;

/// The workspace `settings.json` after the merge, parsed.
fn read(workspace: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(workspace.join(".claude/settings.json")).unwrap())
        .unwrap()
}

fn seed(workspace: &std::path::Path, body: &str) {
    fs::create_dir_all(workspace.join(".claude")).unwrap();
    fs::write(workspace.join(".claude/settings.json"), body).unwrap();
}

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

    let body = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
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
    let body = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
    assert!(!body.contains("arvo--gone.sh"), "{body}");
    assert!(body.contains("arvo--theirs.sh"), "{body}");
}
