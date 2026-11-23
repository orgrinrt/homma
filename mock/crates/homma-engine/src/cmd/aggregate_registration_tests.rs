//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a registration names its hook and the program it runs it with, and
//! which files under `.claude/hooks/` are hooks at all.

use std::fs;

use super::chains_tests::{QUIET, plant_repo, regen};
use super::wrapper::HookCall;
use super::*;

/// Every `(run, runner, args, matcher)` among `entries`, sorted.
fn carried(entries: &[crate::cmd::dispatch::Entry]) -> Vec<(String, String, String, String)> {
    let mut out: Vec<_> = entries
        .iter()
        .map(|e| {
            (
                e.run.clone(),
                e.runner.clone(),
                e.args.clone(),
                e.matcher.clone(),
            )
        })
        .collect();
    out.sort();
    out
}

fn four(run: &str, runner: &str, args: &str, matcher: &str) -> (String, String, String, String) {
    (run.into(), runner.into(), args.into(), matcher.into())
}

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
    let marker = ws.path().join("fired");
    let repo = plant_repo(
        ws.path(),
        &[],
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"bash .claude/hooks/check.sh --strict"}]}]}}"#,
    );
    // No shebang and no execute bit: only `bash` runs this.
    fs::write(
        repo.join(".claude/hooks/check.sh"),
        format!(
            "cat > /dev/null\nprintf '%s' \"$*\" > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let (a, entries, _) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(carried(&entries), vec![four(
        "arvo/.claude/hooks/check.sh",
        "bash",
        "--strict",
        "Bash"
    )]);

    let payload = format!(
        r#"{{"tool_name":"Bash","cwd":"{}","tool_input":{{"command":"ls"}}}}"#,
        repo.display()
    );
    crate::cmd::dispatch::tests::dispatch(
        &ws.path().join(".claude/hooks/_dispatch.sh"),
        &["PreToolUse"],
        &payload,
        None,
        &[],
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "--strict");
}

#[test]
fn a_hook_two_registrations_run_two_ways_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("check.sh", QUIET), ("other.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":"bash .claude/hooks/check.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/check.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"sh .claude/hooks/other.sh"}]}
        ]}}"#,
    );
    let (a, entries, _) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(
        a.problems[0].contains("arvo/.claude/hooks/check.sh"),
        "{:?}",
        a.problems
    );
    assert!(
        a.problems[0].contains("itself and `bash`"),
        "{:?}",
        a.problems
    );
    // The control: a hook every registration runs one way is carried.
    assert_eq!(carried(&entries), vec![four(
        "arvo/.claude/hooks/other.sh",
        "sh",
        "",
        "Edit"
    )]);
}

#[test]
fn a_hook_run_through_a_line_of_shell_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("check.sh", QUIET), ("other.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":"cd . && .claude/hooks/check.sh"}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":"bash .claude/hooks/other.sh"}]}
        ]}}"#,
    );
    let (a, entries, _) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(
        a.problems[0].contains("arvo/.claude/hooks/check.sh")
            && a.problems[0].contains("a line of shell"),
        "{:?}",
        a.problems
    );
    // The control: the hook run through a program beside it is carried.
    assert_eq!(carried(&entries), vec![four(
        "arvo/.claude/hooks/other.sh",
        "bash",
        "",
        "Edit"
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
    let (a, entries, _) = regen(ws.path(), &repo);
    assert!(a.problems.is_empty(), "{:?}", a.problems);
    assert_eq!(a.hooks, 1);
    assert_eq!(carried(&entries), vec![four(
        "arvo/.claude/hooks/guard.sh",
        "",
        "",
        "Bash"
    )]);
}

#[test]
fn a_hook_given_a_line_of_shell_after_it_is_reported_and_not_carried() {
    for args in ["|| true", "2>/dev/null", "; echo x", "$(date)", "`date`", "& wait"] {
        let ws = tempfile::tempdir().unwrap();
        let repo = plant_repo(
            ws.path(),
            &[("check.sh", QUIET), ("other.sh", QUIET)],
            &format!(
                r#"{{"hooks":{{"PreToolUse":[
                {{"matcher":"Bash","hooks":[{{"type":"command","command":".claude/hooks/check.sh {args}"}}]}},
                {{"matcher":"Edit","hooks":[{{"type":"command","command":".claude/hooks/other.sh --strict 'two words' \"$X\""}}]}}
            ]}}}}"#
            ),
        );
        let (a, entries, _) = regen(ws.path(), &repo);
        assert_eq!(a.problems.len(), 1, "`{args}`: {:?}", a.problems);
        assert!(
            a.problems[0].contains("arvo/.claude/hooks/check.sh")
                && a.problems[0].contains("a line of shell rather than arguments"),
            "`{args}`: {:?}",
            a.problems
        );
        // The control: quoted words and a variable are arguments, and carried.
        assert_eq!(carried(&entries), vec![four(
            "arvo/.claude/hooks/other.sh",
            "",
            "--strict 'two words' \"$X\"",
            "Edit"
        )]);
    }
}

#[test]
fn a_hook_registered_with_a_field_the_dispatcher_does_not_carry_is_reported() {
    let ws = tempfile::tempdir().unwrap();
    let repo = plant_repo(
        ws.path(),
        &[("check.sh", QUIET), ("other.sh", QUIET)],
        r#"{"hooks":{"PreToolUse":[
            {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/check.sh","if":"Bash(git *)","asyncRewake":true}]},
            {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/other.sh","timeout":5,"async":true}]}
        ]}}"#,
    );
    let (a, entries, _) = regen(ws.path(), &repo);
    assert_eq!(a.problems.len(), 1, "{:?}", a.problems);
    assert!(
        a.problems[0].contains("arvo/.claude/hooks/check.sh")
            && a.problems[0].contains("`asyncRewake`")
            && a.problems[0].contains("`if`"),
        "{:?}",
        a.problems
    );
    // The control: the fields it carries are carried.
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].timeout, entries[0].background), (5, true));
}
