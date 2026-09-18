//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A repository's hooks reach the workspace kept at their events and tools,
//! and nothing homma did not write is replaced or removed.

use std::fs;
use std::path::{Path, PathBuf};

use super::tests::{make_executable, test_root};
use super::*;

/// Run a wrapper with `payload` on stdin and `args` on its command line,
/// keeping what it said and how it exited.
pub(crate) fn run_with_payload(
    wrapper: &Path,
    payload: &str,
    args: &[&str],
) -> std::process::Output {
    use std::io::Write;
    let mut child = std::process::Command::new("bash")
        .arg(wrapper)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// A hook that swallows its input and does nothing else.
pub(crate) const QUIET: &str = "#!/usr/bin/env bash\ncat > /dev/null\n";

/// A hook that records its arguments in `marker`, so whether it ran, and with
/// what, is observable rather than read off an exit code that is 0 either way.
fn recording(marker: &Path) -> String {
    format!(
        "#!/usr/bin/env bash\ncat > /dev/null\nprintf '%s' \"$*\" > '{}'\n",
        marker.display()
    )
}

/// A repository `arvo` under `ws`, with each `(name, body)` as an executable
/// hook and `settings` as its `settings.json`.
pub(crate) fn plant_repo(ws: &Path, hooks: &[(&str, &str)], settings: &str) -> PathBuf {
    let repo = ws.join("arvo");
    fs::create_dir_all(repo.join(".claude/hooks")).unwrap();
    for (name, body) in hooks {
        let p = repo.join(".claude/hooks").join(name);
        fs::write(&p, body).unwrap();
        make_executable(&p);
    }
    fs::write(repo.join(".claude/settings.json"), settings).unwrap();
    repo
}

/// The pass over the planted repo and the merge after it, returning what the
/// pass reported and the workspace settings as written.
pub(crate) fn regen(ws: &Path, repo: &Path) -> (Aggregated, serde_json::Value) {
    let root = test_root(ws);
    let mut entries = Vec::new();
    let a = aggregate_repo(&root, "arvo", repo, &mut entries).unwrap();
    merge_settings(&root, &["arvo"], &["arvo"], &entries, None).unwrap();
    (a, settings(ws))
}

fn settings(ws: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(ws.join(".claude/settings.json")).unwrap()).unwrap()
}

/// Every `(event, matcher, command)` a settings value registers, the matcher
/// `None` where the entry names none.
pub(crate) fn registrations(v: &serde_json::Value) -> Vec<(String, Option<String>, String)> {
    let mut out = Vec::new();
    for (event, arr) in v["hooks"].as_object().unwrap() {
        for e in arr.as_array().unwrap() {
            let matcher = e
                .get("matcher")
                .and_then(|m| m.as_str())
                .map(str::to_string);
            if let Some(hs) = e["hooks"].as_array() {
                for h in hs {
                    out.push((
                        event.clone(),
                        matcher.clone(),
                        h["command"].as_str().unwrap().to_string(),
                    ));
                }
            }
        }
    }
    out
}

pub(crate) fn reg(
    event: &str,
    matcher: Option<&str>,
    file: &str,
) -> (String, Option<String>, String) {
    (
        event.to_string(),
        matcher.map(str::to_string),
        format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{file}"),
    )
}

fn workspace_settings(ws: &Path, body: &str) {
    fs::create_dir_all(ws.join(".claude/hooks")).unwrap();
    fs::write(ws.join(".claude/settings.json"), body).unwrap();
}

#[test]
fn every_event_and_matcher_a_repository_gives_a_hook_is_carried_to_it() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", QUIET), ("at-the-end.sh", QUIET)],
        r#"{"hooks":{
            "PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}],
            "PostToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}],
            "Stop":[{"hooks":[{"type":"command","command":".claude/hooks/at-the-end.sh"}]}]
        }}"#,
    );
    let (a, v) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(a.hooks, 2);

    let mut got = registrations(&v);
    got.sort();
    let mut want = vec![
        reg("PreToolUse", Some("Edit"), "arvo--guard.sh"),
        reg("PostToolUse", Some("Write"), "arvo--guard.sh"),
        reg("Stop", None, "arvo--at-the-end.sh"),
    ];
    want.sort();
    // Exactly these: an event folded into another, a matcher invented for a
    // registration that named none, or a registration dropped all show here.
    assert_eq!(got, want);
    assert!(
        v["hooks"]["Stop"][0].get("matcher").is_none(),
        "a registration that named no matcher was given one: {v}"
    );
}

#[test]
fn a_hook_named_only_by_its_own_matchers_line_is_carried_to_pre_tool_use() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", "#!/usr/bin/env bash\n# @matchers: Bash, Edit\n")],
        "{}",
    );
    let (a, v) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    let mut got = registrations(&v);
    got.sort();
    assert_eq!(got, vec![
        reg("PreToolUse", Some("Bash"), "arvo--guard.sh"),
        reg("PreToolUse", Some("Edit"), "arvo--guard.sh"),
    ]);
}

#[test]
fn a_hook_nothing_calls_is_reported_by_path_and_no_wrapper_is_written() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", QUIET), ("orphan.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    );
    let (a, v) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(a.problems[0].contains("arvo/.claude/hooks/orphan.sh"));
    assert!(a.problems[0].contains("nothing would call it"));
    assert!(!ws.path().join(".claude/hooks/arvo--orphan.sh").exists());
    // The control: its registered sibling in the same pass is carried.
    assert!(ws.path().join(".claude/hooks/arvo--guard.sh").exists());
    assert_eq!(registrations(&v), vec![reg(
        "PreToolUse",
        Some("Bash"),
        "arvo--guard.sh"
    )]);
}

#[test]
fn a_hook_the_host_could_not_run_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    let body = r#"{"hooks":{"PreToolUse":[
        {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]},
        {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/plain.sh"}]}
    ]}}"#;
    let repo = plant_repo(ws.path(), &[("guard.sh", QUIET)], body);
    fs::write(repo.join(".claude/hooks/plain.sh"), QUIET).unwrap();
    let (a, v) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(a.problems[0].contains("arvo/.claude/hooks/plain.sh"));
    assert!(a.problems[0].contains("not executable"));
    assert!(!ws.path().join(".claude/hooks/arvo--plain.sh").exists());
    assert_eq!(registrations(&v), vec![reg(
        "PreToolUse",
        Some("Bash"),
        "arvo--guard.sh"
    )]);
}

#[test]
fn a_hook_in_any_language_is_carried_and_runs() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    let repo = plant_repo(
        ws.path(),
        &[("guard", &recording(&marker))],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard"}]}]}}"#,
    );
    let (a, v) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(registrations(&v), vec![reg(
        "PreToolUse",
        Some("Edit"),
        "arvo--guard.sh"
    )]);
    let wrapper = ws.path().join(".claude/hooks/arvo--guard.sh");
    let payload = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        repo.join("src/lib.rs").display()
    );
    run_with_payload(&wrapper, &payload, &[]);
    assert!(marker.exists(), "the carried hook did not run");
}

#[test]
fn the_arguments_a_registration_carries_reach_the_hook() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", &recording(&marker))],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.sh --strict"}]}]}}"#,
    );
    let (a, v) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    let (_, _, cmd) = &registrations(&v)[0];
    assert!(cmd.ends_with("arvo--guard.sh --strict"), "{cmd}");

    // What the host does with that command: the wrapper runs with the
    // argument, and has to pass it on.
    let wrapper = ws.path().join(".claude/hooks/arvo--guard.sh");
    let payload = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        repo.join("src/lib.rs").display()
    );
    run_with_payload(&wrapper, &payload, &["--strict"]);
    assert_eq!(fs::read_to_string(&marker).unwrap(), "--strict");
}

#[test]
fn a_file_homma_did_not_write_is_neither_overwritten_nor_unregistered() {
    let ws = tempfile::tempdir().unwrap();
    workspace_settings(
        ws.path(),
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--guard.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--gone.sh"}]}
        ]}}"#,
    );
    let mine = "#!/bin/sh\n# somebody's own guard\n";
    let theirs = ws.path().join(".claude/hooks/arvo--guard.sh");
    fs::write(&theirs, mine).unwrap();
    // A wrapper homma wrote on an earlier run for a hook the repo has since
    // dropped: this one is homma's to remove, which is the control.
    let gone = ws.path().join(".claude/hooks/arvo--gone.sh");
    fs::write(
        &gone,
        wrapper_script("arvo", "arvo", ".claude/hooks/gone.sh", ""),
    )
    .unwrap();

    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    );
    let (a, v) = regen(ws.path(), &repo);

    assert_eq!(fs::read_to_string(&theirs).unwrap(), mine);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(a.problems[0].contains(".claude/hooks/arvo--guard.sh"));
    assert!(a.problems[0].contains("not written by homma"));
    // Its own registration is kept as it was, and none is added for the repo
    // hook that could not be carried over it.
    assert_eq!(registrations(&v), vec![reg(
        "PreToolUse",
        Some("Bash"),
        "arvo--guard.sh"
    )]);
    assert!(!gone.exists(), "a stale wrapper homma wrote survived");
}

#[test]
fn a_file_under_a_repositorys_prefix_that_homma_did_not_write_survives_the_sweep() {
    let ws = tempfile::tempdir().unwrap();
    workspace_settings(
        ws.path(),
        r#"{"hooks":{"PostToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":".claude/hooks/arvo--mine.sh"}]}]}}"#,
    );
    let mine = ws.path().join(".claude/hooks/arvo--mine.sh");
    fs::write(&mine, "#!/bin/sh\n").unwrap();
    let repo = plant_repo(ws.path(), &[], "{}");
    let (a, v) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert!(mine.exists());
    assert_eq!(registrations(&v), vec![(
        "PostToolUse".to_string(),
        Some("Write".to_string()),
        ".claude/hooks/arvo--mine.sh".to_string()
    )]);
}

#[test]
fn every_event_is_swept_of_homma_registrations_and_of_nothing_else() {
    let ws = tempfile::tempdir().unwrap();
    workspace_settings(
        ws.path(),
        r#"{"hooks":{
            "PostToolUse":[{"matcher":"Write","hooks":[
                {"type":"command","command":".claude/hooks/arvo--old.sh"},
                {"type":"command","command":".claude/hooks/hand.sh"}
            ]}],
            "Stop":[{"hooks":[{"type":"command","command":".claude/hooks/arvo--old2.sh"}]}],
            "Notification":[],
            "SessionStart":[{"matcher":"startup"}]
        }}"#,
    );
    merge_settings(&test_root(ws.path()), &["arvo"], &["arvo"], &[], None).unwrap();
    let v = settings(ws.path());
    assert_eq!(registrations(&v), vec![(
        "PostToolUse".to_string(),
        Some("Write".to_string()),
        ".claude/hooks/hand.sh".to_string()
    )]);
    assert!(
        v["hooks"].get("Stop").is_none(),
        "an event this sweep emptied stayed: {v}"
    );
    assert_eq!(
        v["hooks"]["Notification"],
        serde_json::json!([]),
        "an empty event somebody wrote was removed: {v}"
    );
    assert_eq!(
        v["hooks"]["SessionStart"],
        serde_json::json!([{"matcher":"startup"}]),
        "an entry homma cannot read was removed: {v}"
    );
}

#[test]
fn the_mark_is_what_makes_a_file_hommas() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("h.sh");

    fs::write(&p, wrapper_script("arvo", "arvo", ".claude/hooks/x.sh", "")).unwrap();
    assert!(carries_the_mark(&p), "an aggregated wrapper");

    fs::write(
        &p,
        "#!/bin/sh\n\n\n\n\n\n# written by `homma agent regen`\n",
    )
    .unwrap();
    assert!(!carries_the_mark(&p), "the words past the first five lines");

    fs::write(&p, "#!/bin/sh\necho written by `homma agent regen`\n").unwrap();
    assert!(!carries_the_mark(&p), "the words outside a comment");

    fs::write(&p, "#!/bin/sh\n# written by homma\n").unwrap();
    assert!(!carries_the_mark(&p), "a near miss");

    assert!(!carries_the_mark(&dir.path().join("absent")));

    let ws = tempfile::tempdir().unwrap();
    crate::cmd::gates::install_workspace_gate(&test_root(ws.path()), &[]).unwrap();
    assert!(
        carries_the_mark(
            &ws.path()
                .join(".claude/hooks/_workspace--mockspace-gate.sh")
        ),
        "the workspace gate"
    );
}

#[test]
fn a_shell_call_is_narrowed_by_the_directory_the_host_says_the_session_is_in() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", &recording(&marker))],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    );
    regen(ws.path(), &repo);
    let wrapper = ws.path().join(".claude/hooks/arvo--guard.sh");
    let inside = repo.join("src");
    let outside = ws.path().join("kolli");

    let call = |cwd: &Path| {
        format!(
            r#"{{"cwd":"{}","tool_input":{{"command":"cargo test"}}}}"#,
            cwd.display()
        )
    };
    run_with_payload(&wrapper, &call(&inside), &[]);
    assert!(
        marker.exists(),
        "a shell call in the repo skipped the repo's hook"
    );

    fs::remove_file(&marker).unwrap();
    run_with_payload(&wrapper, &call(&outside), &[]);
    assert!(
        !marker.exists(),
        "a shell call elsewhere ran the repo's hook"
    );

    // For a file tool the file decides, wherever the session is.
    let write_elsewhere = format!(
        r#"{{"cwd":"{}","tool_input":{{"file_path":"{}"}}}}"#,
        inside.display(),
        outside.join("x.rs").display()
    );
    run_with_payload(&wrapper, &write_elsewhere, &[]);
    assert!(
        !marker.exists(),
        "a write outside the repo ran its hook because the session was inside it"
    );
}

#[test]
fn a_file_under_the_gates_name_that_homma_did_not_write_is_refused_and_left() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    let gate = ws
        .path()
        .join(".claude/hooks/_workspace--mockspace-gate.sh");
    let mine = "#!/bin/sh\n# somebody's own\n";
    fs::write(&gate, mine).unwrap();

    let err = crate::cmd::gates::install_workspace_gate(&test_root(ws.path()), &[]).unwrap_err();
    assert!(err.to_string().contains("not written by homma"), "{err}");
    assert_eq!(fs::read_to_string(&gate).unwrap(), mine);

    // The control: with the file gone the gate is written.
    fs::remove_file(&gate).unwrap();
    crate::cmd::gates::install_workspace_gate(&test_root(ws.path()), &[]).unwrap();
    assert!(carries_the_mark(&gate));
}
