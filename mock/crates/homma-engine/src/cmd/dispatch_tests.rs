//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The table and the registrations, and the helpers every dispatcher test runs
//! the written script with.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::cmd::aggregate::tests::{make_executable, test_root};

/// An entry of `event` and `matcher` running `run`, narrowed to `roots`, with
/// the default timeout, waited on.
pub(in crate::cmd) fn entry(event: &str, matcher: &str, run: &str, roots: &[&str]) -> Entry {
    Entry {
        event:      event.to_string(),
        matcher:    matcher.to_string(),
        run:        run.to_string(),
        runner:     String::new(),
        args:       String::new(),
        roots:      roots.iter().map(|r| r.to_string()).collect(),
        timeout:    homma_api::DEFAULT_TIMEOUT,
        background: false,
        from:       "arvo".to_string(),
    }
}

/// Write `entries` and the dispatcher into `ws`, as the regen does, and return
/// the dispatcher.
pub(in crate::cmd) fn plant(ws: &Path, entries: &[Entry], filter: &[String]) -> PathBuf {
    install(&test_root(ws), entries, filter).unwrap();
    ws.join(".claude/hooks").join(DISPATCHER)
}

/// An executable hook at `rel` under `ws` with `body` after the shebang.
pub(in crate::cmd) fn hook(ws: &Path, rel: &str, body: &str) -> PathBuf {
    let p = ws.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    make_executable(&p);
    p
}

/// Run the dispatcher with `args`, `payload` on its stdin, in `cwd` where given,
/// each `(name, Some(value))` set in its environment.
pub(in crate::cmd) fn dispatch(
    dispatcher: &Path,
    args: &[&str],
    payload: &str,
    cwd: Option<&Path>,
    env: &[(&str, Option<&str>)],
) -> std::process::Output {
    use std::io::Write;
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(dispatcher)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in env {
        if let Some(v) = v {
            cmd.env(k, v);
        }
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

pub(in crate::cmd) fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn command(tail: &str) -> String {
    format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/_dispatch.sh {tail}")
}

#[test]
fn a_folded_event_gets_one_registration_for_what_waits_and_one_for_what_does_not() {
    let mut late = entry("PostToolUse", "Edit", "a", &[]);
    late.background = true;
    let regs = registrations(&[
        entry("PreToolUse", "Bash", "a", &[]),
        entry("PreToolUse", "Edit|Write", "b", &[]),
        entry("PreToolUse", "Bash", "c", &[]),
        entry("PostToolUse", "Edit", "d", &[]),
        late,
    ]);
    let shown: Vec<(&str, &str, String, bool)> = regs
        .iter()
        .map(|r| {
            (
                r.event.as_str(),
                r.matcher.as_str(),
                r.command.clone(),
                r.background,
            )
        })
        .collect();
    assert_eq!(shown, vec![
        ("PostToolUse", "Edit", command("PostToolUse sync"), false),
        ("PostToolUse", "Edit", command("PostToolUse async"), true),
        (
            "PreToolUse",
            "Bash|Edit|Write",
            command("PreToolUse sync"),
            false
        ),
    ]);
}

#[test]
fn any_other_event_gets_a_registration_per_entry_under_its_own_matcher_and_time() {
    let mut late = entry("Stop", "", "b", &[]);
    late.background = true;
    late.timeout = 5;
    let regs = registrations(&[
        entry("PreToolUse", "Bash", "a", &[]),
        entry("Notification", "idle_prompt", "n", &[]),
        late,
        entry("Stop", "*", "c", &[]),
    ]);
    let shown: Vec<(&str, &str, String, Option<u32>, bool)> = regs
        .iter()
        .map(|r| {
            (
                r.event.as_str(),
                r.matcher.as_str(),
                r.command.clone(),
                r.timeout,
                r.background,
            )
        })
        .collect();
    // the entry is named by its place in the table, which is written with them
    assert_eq!(shown, vec![
        (
            "PreToolUse",
            "Bash",
            command("PreToolUse sync"),
            Some(homma_api::DEFAULT_TIMEOUT + 10),
            false
        ),
        (
            "Notification",
            "idle_prompt",
            command("Notification one 1"),
            Some(homma_api::DEFAULT_TIMEOUT),
            false
        ),
        ("Stop", "", command("Stop one 2"), Some(5), true),
        (
            "Stop",
            "*",
            command("Stop one 3"),
            Some(homma_api::DEFAULT_TIMEOUT),
            false
        ),
    ]);
}

#[test]
fn the_joined_matcher_names_each_alternative_once() {
    let regs = registrations(&[
        entry("PreToolUse", "Bash", "a", &[]),
        entry("PreToolUse", "Bash|mcp__.*", "b", &[]),
        entry("PreToolUse", "Edit, Write", "c", &[]),
        entry(
            "PreToolUse",
            "mcp__[^_]+(_[^_]+)*__(add_repo|register_repo_root)",
            "d",
            &[],
        ),
        entry("PreToolUse", "Agent|Bash", "e", &[]),
    ]);
    assert_eq!(
        regs[0].matcher,
        "Bash|mcp__.*|Edit|Write|mcp__[^_]+(_[^_]+)*__(add_repo|register_repo_root)|Agent"
    );
}

/// Whether `matcher` takes `tool` the way the host reads a matcher: every tool
/// for `*` or none, a list of exact names where it holds only name characters,
/// a regular expression otherwise, here through `jq`'s.
fn host_takes(matcher: &str, tool: &str) -> bool {
    if matcher.is_empty() || matcher == "*" {
        return true;
    }
    if matcher
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ' | ',' | '|'))
    {
        return matcher.split(['|', ',']).map(str::trim).any(|n| n == tool);
    }
    let out = std::process::Command::new("jq")
        .args(["-n", "-e", "--arg", "m", matcher, "--arg", "t", tool, "$t | test($m)"])
        .output()
        .unwrap();
    out.status.success()
}

#[test]
fn the_joined_matcher_takes_every_tool_any_entry_takes() {
    let matchers = [
        "Edit, Write",
        "mcp__.*",
        "Bash",
        "mcp__[^_]+(_[^_]+)*__(add_repo|register_repo_root)",
        "^Read$|",
        "Agent|Task",
    ];
    let tools = [
        "Edit",
        "Write",
        "Bash",
        "Read",
        "Grep",
        "Agent",
        "Task",
        "NotebookEdit",
        "mcp__claude-code-remote__add_repo",
        "mcp__x__get_session",
    ];
    // Every pair of matchers, so a join that loses a list spelled with `,` or an
    // empty branch beside another entry shows.
    for a in matchers {
        for b in matchers {
            let regs = registrations(&[
                entry("PreToolUse", a, "x", &[]),
                entry("PreToolUse", b, "y", &[]),
            ]);
            let joined = &regs[0].matcher;
            for t in tools {
                if host_takes(a, t) || host_takes(b, t) {
                    assert!(
                        host_takes(joined, t),
                        "`{a}` and `{b}` joined as `{joined}` do not take `{t}`"
                    );
                }
            }
        }
    }
    // the control: the join of whole matchers, the shape this replaced, loses one
    assert!(host_takes("Edit, Write", "Write"));
    assert!(!host_takes("Edit, Write|mcp__.*", "Write"));
}

#[test]
fn one_entry_taking_every_tool_makes_the_registration_take_every_tool() {
    for every in ["*", ""] {
        let regs = registrations(&[
            entry("PreToolUse", "Bash", "a", &[]),
            entry("PreToolUse", every, "b", &[]),
        ]);
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].matcher, "*", "`{every}`");
    }
    // where no entry names one, the registration names none either
    let regs =
        registrations(&[entry("PostToolUse", "", "a", &[]), entry("PostToolUse", "", "b", &[])]);
    assert_eq!(regs[0].matcher, "");
}

#[test]
fn the_registration_gives_the_longest_entry_ten_seconds_over() {
    let mut slow = entry("PreToolUse", "*", "a", &[]);
    slow.timeout = 300;
    let regs = registrations(&[entry("PreToolUse", "*", "b", &[]), slow]);
    assert_eq!(regs[0].timeout, Some(310));
    // the control: the default alone
    let regs = registrations(&[entry("PreToolUse", "*", "b", &[])]);
    assert_eq!(regs[0].timeout, Some(homma_api::DEFAULT_TIMEOUT + 10));
    // and the most there is does not wrap
    let mut most = entry("PreToolUse", "*", "c", &[]);
    most.timeout = u32::MAX;
    assert_eq!(registrations(&[most])[0].timeout, Some(u32::MAX));
}

#[test]
fn the_registration_as_the_host_reads_it_carries_its_timeout_and_async() {
    let mut late = entry("PostToolUse", "Edit", "a", &[]);
    late.background = true;
    let regs = registrations(&[late]);
    let ws = tempfile::tempdir().unwrap();
    crate::cmd::aggregate::merge_settings(&test_root(ws.path()), &[], &regs).unwrap();
    let v: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(ws.path().join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let h = &v["hooks"]["PostToolUse"][0]["hooks"][0];
    assert_eq!(h["async"], serde_json::json!(true));
    assert_eq!(
        h["timeout"],
        serde_json::json!(homma_api::DEFAULT_TIMEOUT + 10)
    );
    assert_eq!(
        v["hooks"]["PostToolUse"][0]["matcher"],
        serde_json::json!("Edit")
    );
}

#[test]
fn the_table_reads_back_as_written_and_one_homma_did_not_write_reads_as_none() {
    let ws = tempfile::tempdir().unwrap();
    let written = vec![entry("PreToolUse", "Bash", "arvo/.claude/hooks/a.sh", &["arvo"])];
    plant(ws.path(), &written, &[]);
    let root = test_root(ws.path());
    assert_eq!(previous(&root), written);

    fs::write(
        ws.path().join(".claude/hooks").join(TABLE),
        r#"{"written":"by hand","entries":[]}"#,
    )
    .unwrap();
    assert!(previous(&root).is_empty());
    // and is not written over
    let err = install(&root, &written, &[]).unwrap_err().to_string();
    assert!(err.contains("not written by homma"), "{err}");
}

#[test]
fn a_dispatcher_homma_did_not_write_is_not_written_over() {
    let ws = tempfile::tempdir().unwrap();
    let hooks = ws.path().join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join(DISPATCHER), "#!/bin/sh\n# mine\n").unwrap();
    let root = test_root(ws.path());
    let err = install(&root, &[], &[]).unwrap_err().to_string();
    assert!(err.contains("not written by homma"), "{err}");
    assert_eq!(
        fs::read_to_string(hooks.join(DISPATCHER)).unwrap(),
        "#!/bin/sh\n# mine\n"
    );
}

#[test]
fn every_wrapper_an_earlier_regen_wrote_is_retired_and_nothing_else() {
    let ws = tempfile::tempdir().unwrap();
    let hooks = ws.path().join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let marked = "#!/usr/bin/env bash\n# Aggregated from `arvo` by `homma agent regen`.\n";
    for old in ["arvo--foo.sh", "_declared--Stop--x.sh", "kolli--bar.sh"] {
        fs::write(hooks.join(old), marked).unwrap();
    }
    fs::write(
        hooks.join("mine--guard.sh"),
        "#!/usr/bin/env bash\n# by hand\n",
    )
    .unwrap();
    fs::write(hooks.join(crate::cmd::gates::GATE_SCRIPT_NAME), marked).unwrap();
    plant(ws.path(), &[], &[]);
    // writing the dispatcher leaves them: the settings still name them
    assert!(
        hooks.join("arvo--foo.sh").exists(),
        "retired before the settings moved"
    );

    retire_wrappers(&test_root(ws.path())).unwrap();
    for old in ["arvo--foo.sh", "_declared--Stop--x.sh", "kolli--bar.sh"] {
        assert!(!hooks.join(old).exists(), "{old} was left");
    }
    assert!(
        hooks.join("mine--guard.sh").exists(),
        "a hand-written hook was removed"
    );
    assert!(
        hooks.join(crate::cmd::gates::GATE_SCRIPT_NAME).exists(),
        "the workspace gate was removed"
    );
    assert!(hooks.join(DISPATCHER).exists());
}

#[test]
fn the_dispatcher_names_no_machine_and_finds_its_workspace_from_where_it_sits() {
    let s = dispatcher_script(&["tools/dedup/dedup".into(), "run".into()]);
    assert!(s.starts_with("#!/usr/bin/env bash"));
    assert!(
        s.lines()
            .take(5)
            .any(|l| l.starts_with('#') && l.contains(crate::cmd::aggregate::MANAGED_MARK)),
        "the dispatcher does not carry the mark"
    );
    assert!(s.contains("FILTER=(\"$WS\"/'tools/dedup/dedup' 'run')"));
    for line in s.lines() {
        let code = line.split('#').next().unwrap_or(line);
        assert!(
            !code.contains("='/") && !code.contains("=\"/"),
            "the dispatcher assigns an absolute path: {line}"
        );
    }
    // the control: no filter writes an empty one
    assert!(dispatcher_script(&[]).contains("FILTER=()"));
}
