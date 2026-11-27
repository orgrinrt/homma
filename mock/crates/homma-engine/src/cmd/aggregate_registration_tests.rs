//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a registration names its hook and the program it runs it with, and
//! which files under `.claude/hooks/` are hooks at all.

use std::fs;

use super::call::HookCall;
use super::chains_tests::{QUIET, hook, plant_repo, regen, shapes};
use super::*;

#[test]
fn a_hooks_file_is_read_off_every_spelling_of_its_path() {
    for (cmd, want) in [
        (".claude/hooks/a.sh", Some("a.sh")),
        ("\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/a.sh", Some("a.sh")),
        ("/abs/ws/.claude/hooks/a.sh --strict", Some("a.sh")),
        ("'.claude/hooks/a'", Some("a")),
        ("bash .claude/hooks/a.sh", Some("a.sh")),
        ("scripts/a.sh", None),
        ("bash scripts/a.sh .claude/hooks", None),
        (".claude/hooks/sub/a.sh", None),
        (".claude/hooks/", None),
        ("", None),
    ] {
        assert_eq!(hook_file_named(cmd).as_deref(), want, "{cmd}");
    }
}

#[test]
fn a_registration_keeps_the_program_before_the_hook_and_the_arguments_after_it() {
    let call = |cmd: &str| hook_call(cmd).unwrap();
    assert_eq!(call(".claude/hooks/a.sh"), HookCall {
        name:   "a.sh".into(),
        runner: String::new(),
        args:   String::new(),
    });
    assert_eq!(call("bash .claude/hooks/a.sh --strict"), HookCall {
        name:   "a.sh".into(),
        runner: "bash".into(),
        args:   "--strict".into(),
    });
    assert_eq!(
        call("  env A=1 node \"$CLAUDE_PROJECT_DIR\"/.claude/hooks/a.js 'x  y'  "),
        HookCall {
            name:   "a.js".into(),
            runner: "env A=1 node".into(),
            args:   "'x  y'".into(),
        }
    );
}
#[test]
fn a_hook_a_registration_runs_through_a_program_is_carried_without_an_execute_bit() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"bash .claude/hooks/check.sh --strict"}]}]}}"#,
    );
    // No shebang and no execute bit: only `bash` runs this.
    fs::write(repo.join(".claude/hooks/check.sh"), "cat > /dev/null\n").unwrap();
    let (a, rows) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(shapes(&rows), vec![(
        "PreToolUse".to_string(),
        "Bash".to_string(),
        vec!["bash".to_string(), hook("check.sh"), "--strict".to_string()],
    )]);
}

#[test]
fn a_hook_two_registrations_run_two_ways_is_a_row_for_each() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("check.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/check.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"bash .claude/hooks/check.sh"}]}
        ]}}"#,
    );
    let (a, rows) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(shapes(&rows), vec![
        ("PreToolUse".to_string(), "Bash".to_string(), vec![hook(
            "check.sh"
        )]),
        ("PreToolUse".to_string(), "Edit".to_string(), vec![
            "bash".to_string(),
            hook("check.sh")
        ]),
    ]);
}

#[test]
fn a_hook_run_through_a_line_of_shell_or_quoted_arguments_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("check.sh", QUIET), ("quoted.sh", QUIET), ("other.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":"cd . && .claude/hooks/check.sh"}]},
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/quoted.sh 'a b'"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"bash .claude/hooks/other.sh"}]}
        ]}}"#,
    );
    let (a, rows) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 2, "{:?}", a.problems);
    assert!(
        a.problems
            .iter()
            .any(|p| p.contains("check.sh") && p.contains("a line of shell"))
    );
    assert!(
        a.problems
            .iter()
            .any(|p| p.contains("quoted.sh") && p.contains("would not be what the host"))
    );
    // The control: the hook run through a program beside them is carried.
    assert_eq!(shapes(&rows), vec![(
        "PreToolUse".to_string(),
        "Edit".to_string(),
        vec!["bash".to_string(), hook("other.sh")],
    )]);
}

#[test]
fn a_hidden_file_among_the_hooks_is_neither_carried_nor_reported() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("guard.sh", QUIET), (".guard.sh.swp", QUIET)],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    );
    let (a, rows) = regen(ws.path(), &repo);
    assert_eq!(a.hooks, 1);
    assert!(a.problems.is_empty());
    assert_eq!(rows.len(), 1);
}
