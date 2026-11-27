//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A repository's hooks become rows kept at their events and tools, narrowed
//! to the repository, and nothing homma did not write is replaced or removed.

use std::fs;
use std::path::{Path, PathBuf};

use super::tests::test_root;
use super::*;

/// A hook that swallows its input and does nothing else.
pub(crate) const QUIET: &str = "#!/usr/bin/env bash\ncat > /dev/null\n";

/// A repository `acre` under `ws`, with each `(name, body)` as an executable
/// hook and `settings` as its `settings.json`.
pub(crate) fn plant_repo(ws: &Path, hooks: &[(&str, &str)], settings: &str) -> PathBuf {
    let repo = ws.join("acre");
    fs::create_dir_all(repo.join(".claude/hooks")).unwrap();
    for (name, body) in hooks {
        homma_fixture::script(&repo.join(".claude/hooks").join(name), body, 0o755);
    }
    fs::write(repo.join(".claude/settings.json"), settings).unwrap();
    repo
}

/// The pass over the planted repo: what it reported and the rows it made.
pub(crate) fn regen(ws: &Path, repo: &Path) -> (Aggregated, Vec<Row>) {
    let root = test_root(ws);
    let mut rows = Vec::new();
    let a = aggregate_repo(&root, "acre", repo, &mut rows).unwrap();
    (a, rows)
}

/// Each row as `(event, matcher, run)`.
pub(crate) fn shapes(rows: &[Row]) -> Vec<(String, String, Vec<String>)> {
    rows.iter()
        .map(|r| (r.event.clone(), r.matcher.clone(), r.run.clone()))
        .collect()
}

pub(crate) fn hook(file: &str) -> String {
    format!("{{root}}/acre/.claude/hooks/{file}")
}

#[test]
fn every_event_and_matcher_a_repository_gives_a_hook_is_a_row_narrowed_to_it() {
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
    let (a, rows) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(a.hooks, 2);
    let mut got = shapes(&rows);
    got.sort();
    assert_eq!(got, vec![
        ("PostToolUse".into(), "Write".into(), vec![hook("guard.sh")]),
        ("PreToolUse".into(), "Edit".into(), vec![hook("guard.sh")]),
        ("Stop".into(), String::new(), vec![hook("at-the-end.sh")]),
    ]);
    for r in &rows {
        assert_eq!(r.paths, ["acre"]);
        assert!(r.name.starts_with("acre--"), "{}", r.name);
        assert!(r.filtered && !r.at_root);
    }
}

#[test]
fn a_hook_named_only_by_its_own_matchers_line_is_a_row_at_pre_tool_use() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[(
            "guard.sh",
            "#!/usr/bin/env bash\n# @matchers: Write, Edit\ncat > /dev/null\n",
        )],
        "{}",
    );
    let (_, rows) = regen(ws.path(), &repo);
    assert_eq!(shapes(&rows), vec![
        ("PreToolUse".into(), "Write".into(), vec![hook("guard.sh")]),
        ("PreToolUse".into(), "Edit".into(), vec![hook("guard.sh")]),
    ]);
}

#[test]
fn a_hook_nothing_calls_is_reported_by_path_and_no_row_is_made() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(ws.path(), &[("orphan.sh", QUIET)], "{}");
    let (a, rows) = regen(ws.path(), &repo);
    assert!(rows.is_empty());
    assert_eq!(a.problems.len(), 1);
    assert!(
        a.problems[0].contains("acre/.claude/hooks/orphan.sh"),
        "{:?}",
        a.problems
    );
}

#[test]
fn a_hook_the_host_could_not_run_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/plain.sh"}]}]}}"#,
    );
    fs::write(repo.join(".claude/hooks/plain.sh"), QUIET).unwrap();
    let (a, rows) = regen(ws.path(), &repo);
    assert!(rows.is_empty());
    assert!(a.problems[0].contains("not executable"), "{:?}", a.problems);
}

#[test]
fn the_arguments_and_the_timeout_a_registration_carries_reach_the_row() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.js", "#!/usr/bin/env node\n")],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":"\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/guard.js --strict check","timeout":30}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.js"}]}
        ]}}"#,
    );
    let (a, rows) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(rows[0].run, [
        hook("guard.js"),
        "--strict".into(),
        "check".into()
    ]);
    assert_eq!(rows[0].timeout, 30);
    assert_eq!(rows[1].run, [hook("guard.js")]);
    assert_eq!(rows[1].timeout, crate::cmd::agent_shook::DEFAULT_TIMEOUT);
}

#[test]
fn the_pass_writes_and_removes_nothing_under_the_workspace_hooks() {
    // An older homma's wrapper stays until the front has registered what
    // replaces it, so a refused install leaves the clone with the hooks it had.
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    let old = ws.path().join(".claude/hooks/acre--guard.sh");
    fs::write(&old, format!("#!/bin/sh\n# written {MANAGED_MARK}\n")).unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    );
    regen(ws.path(), &repo);
    let names: Vec<String> = fs::read_dir(ws.path().join(".claude/hooks"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["acre--guard.sh"]);
}

#[test]
fn the_retirement_removes_every_wrapper_homma_wrote_and_nothing_else() {
    let ws = tempfile::tempdir().unwrap();
    let dir = ws.path().join(".claude/hooks");
    fs::create_dir_all(&dir).unwrap();
    let marked = format!("#!/bin/sh\n# written {MANAGED_MARK}\n");
    for name in [
        "acre--guard.sh",
        "_declared--Stop--x.sh",
        "_nudges--Stop.sh",
        "_workspace--mockspace-gate.sh",
    ] {
        fs::write(dir.join(name), &marked).unwrap();
    }
    // somebody's own under homma's shape, and a marked file with no `--`
    fs::write(dir.join("mosaic--mine.sh"), "#!/bin/sh\n").unwrap();
    fs::write(dir.join("plain.sh"), &marked).unwrap();
    retire_wrappers(&test_root(ws.path()), crate::cmd::gates::GATE_SCRIPT_NAME).unwrap();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, [
        "_workspace--mockspace-gate.sh",
        "mosaic--mine.sh",
        "plain.sh"
    ]);
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

    // The control: with the file gone the gate is written, and its row runs it.
    fs::remove_file(&gate).unwrap();
    let row = crate::cmd::gates::install_workspace_gate(&test_root(ws.path()), &[]).unwrap();
    assert!(carries_the_mark(&gate));
    assert_eq!(row.run, [
        "{root}/.claude/hooks/_workspace--mockspace-gate.sh"
    ]);
    assert_eq!(
        (row.event.as_str(), row.matcher.as_str()),
        ("PreToolUse", "Bash")
    );
}
