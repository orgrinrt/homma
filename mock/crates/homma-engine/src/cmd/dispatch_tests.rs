//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The table, the registrations and the dispatcher as a script that runs: what
//! it runs is a property of bash and `jq`, and both are here, so these run what
//! homma writes rather than reading its text.

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

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn an_event_gets_one_registration_for_what_waits_and_one_for_what_does_not() {
    let mut late = entry("PostToolUse", "Edit", "a", &[]);
    late.background = true;
    let regs = registrations(&[
        entry("PreToolUse", "Bash", "a", &[]),
        entry("PreToolUse", "Edit|Write", "b", &[]),
        entry("PreToolUse", "Bash", "c", &[]),
        entry("PostToolUse", "Edit", "d", &[]),
        late,
    ]);
    let shown: Vec<(&str, &str, &str, bool)> = regs
        .iter()
        .map(|r| {
            (
                r.event.as_str(),
                r.matcher.as_str(),
                r.command.as_str(),
                r.background,
            )
        })
        .collect();
    assert_eq!(shown, vec![
        (
            "PostToolUse",
            "Edit",
            "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_dispatch.sh PostToolUse",
            false
        ),
        (
            "PostToolUse",
            "Edit",
            "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_dispatch.sh PostToolUse async",
            true
        ),
        (
            "PreToolUse",
            "Bash|Edit|Write",
            "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_dispatch.sh PreToolUse",
            false
        ),
    ]);
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
    let regs = registrations(&[entry("Stop", "", "a", &[]), entry("Stop", "", "b", &[])]);
    assert_eq!(regs[0].matcher, "");
}

#[test]
fn the_registration_gives_the_longest_entry_ten_seconds_over() {
    let mut slow = entry("Stop", "*", "a", &[]);
    slow.timeout = 300;
    let regs = registrations(&[entry("Stop", "*", "b", &[]), slow]);
    assert_eq!(regs[0].timeout, Some(310));
    // the control: the default alone
    let regs = registrations(&[entry("Stop", "*", "b", &[])]);
    assert_eq!(regs[0].timeout, Some(homma_api::DEFAULT_TIMEOUT + 10));
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
fn every_wrapper_an_earlier_regen_wrote_is_removed_and_nothing_else() {
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

#[test]
fn a_hook_not_in_this_clone_is_passed_over_without_a_word() {
    let ws = tempfile::tempdir().unwrap();
    let d = plant(
        ws.path(),
        &[
            entry("PreToolUse", "*", "arvo/.claude/hooks/gone.sh", &["arvo"]),
            entry("PreToolUse", "*", "arvo/.claude/hooks/gone.sh", &[]),
        ],
        &[],
    );
    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": ws.path().join("arvo/x").display().to_string() },
    })
    .to_string();
    let out = dispatch(&d, &["PreToolUse"], &payload, None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        out.stdout.is_empty() && out.stderr.is_empty(),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn only_the_entries_of_the_event_and_the_mode_run() {
    let ws = tempfile::tempdir().unwrap();
    let marks = ws.path().join("marks");
    fs::create_dir_all(&marks).unwrap();
    let mut entries = Vec::new();
    for (name, event, background) in [
        ("pre", "PreToolUse", false),
        ("post", "PostToolUse", false),
        ("late", "PostToolUse", true),
    ] {
        hook(
            ws.path(),
            &format!("h/{name}"),
            &format!("cat > /dev/null; touch '{}/{name}'", marks.display()),
        );
        let mut e = entry(event, "*", &format!("h/{name}"), &[]);
        e.background = background;
        entries.push(e);
    }
    let d = plant(ws.path(), &entries, &[]);
    let ran = |args: &[&str]| {
        for f in fs::read_dir(&marks).unwrap() {
            fs::remove_file(f.unwrap().path()).unwrap();
        }
        let out = dispatch(&d, args, r#"{"tool_name":"Bash"}"#, None, &[]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        let mut names: Vec<String> = fs::read_dir(&marks)
            .unwrap()
            .map(|f| f.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(ran(&["PreToolUse"]), vec!["pre"]);
    assert_eq!(ran(&["PostToolUse"]), vec!["post"]);
    assert_eq!(ran(&["PostToolUse", "async"]), vec!["late"]);
    assert!(ran(&["Stop"]).is_empty());
}

#[test]
fn a_matcher_is_read_the_way_the_host_reads_one() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    hook(
        ws.path(),
        "h/x",
        &format!("cat > /dev/null; touch '{}'", marker.display()),
    );
    let fires = |matcher: &str, tool: &str| {
        let d = plant(ws.path(), &[entry("PreToolUse", matcher, "h/x", &[])], &[]);
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({ "tool_name": tool }).to_string();
        let out = dispatch(&d, &["PreToolUse"], &payload, None, &[]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        marker.exists()
    };
    // a list of names is exact
    assert!(fires("Edit|Write", "Write"));
    assert!(fires("Edit, Write", "Write"));
    assert!(!fires("Edit|Write", "NotebookEdit"));
    // anything else is an unanchored regular expression
    assert!(fires("Edit.*", "NotebookEdit"));
    assert!(fires(
        "mcp__[^_]+(_[^_]+)*__list_sessions",
        "mcp__claude-code-remote__list_sessions"
    ));
    assert!(!fires(
        "mcp__[^_]+(_[^_]+)*__list_sessions",
        "mcp__x__get_session"
    ));
    // every tool
    assert!(fires("*", "Read"));
    assert!(fires("", "Read"));
}

#[test]
fn a_hook_is_run_through_its_program_with_its_arguments() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    // Not executable, and a shape only bash runs, so whether it ran through the
    // program is what the marker says.
    let real = ws.path().join("arvo/.claude/hooks/check.sh");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(
        &real,
        format!(
            "cat > /dev/null\nprintf '%s|' \"$@\" > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let mut e = entry("PreToolUse", "*", "arvo/.claude/hooks/check.sh", &[]);
    e.runner = "bash".into();
    e.args = "--strict 'two words'".into();
    let d = plant(ws.path(), &[e.clone()], &[]);
    let out = dispatch(&d, &["PreToolUse"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "--strict|two words|");

    // The control: the same file with no program is not run, since it is not
    // executable.
    fs::remove_file(&marker).unwrap();
    e.runner = String::new();
    let d = plant(ws.path(), &[e], &[]);
    let out = dispatch(&d, &["PreToolUse"], "{}", None, &[]);
    assert!(out.status.success());
    assert!(
        !marker.exists(),
        "a file that is not executable ran with no program"
    );
}

#[test]
fn the_hooks_run_side_by_side() {
    let ws = tempfile::tempdir().unwrap();
    // Each waits for the other's mark, so run one after the other the first
    // would time out waiting.
    let a = ws.path().join("a");
    let b = ws.path().join("b");
    let wait_for = |mine: &Path, theirs: &Path| {
        format!(
            "cat > /dev/null; touch '{}'; for _ in $(seq 1 50); do [ -e '{}' ] && exit 0; sleep 0.1; done; exit 1",
            mine.display(),
            theirs.display()
        )
    };
    hook(ws.path(), "h/a", &wait_for(&a, &b));
    hook(ws.path(), "h/b", &wait_for(&b, &a));
    let d = plant(
        ws.path(),
        &[entry("Stop", "*", "h/a", &[]), entry("Stop", "*", "h/b", &[])],
        &[],
    );
    let out = dispatch(&d, &["Stop"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        out.stderr.is_empty(),
        "a hook failed: {}",
        text(&out.stderr)
    );
}

#[test]
fn a_hook_past_its_time_is_stopped_named_and_the_call_goes_on() {
    let ws = tempfile::tempdir().unwrap();
    hook(ws.path(), "h/slow", "cat > /dev/null; sleep 30");
    hook(
        ws.path(),
        "h/quick",
        r#"cat > /dev/null; echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"quick"}}'"#,
    );
    let mut slow = entry("PreToolUse", "*", "h/slow", &[]);
    slow.timeout = 1;
    let d = plant(
        ws.path(),
        &[slow, entry("PreToolUse", "*", "h/quick", &[])],
        &[],
    );
    let started = std::time::Instant::now();
    let out = dispatch(&d, &["PreToolUse"], "{}", None, &[]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the slow hook was waited on"
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("h/slow: ran past its 1 seconds"),
        "{}",
        text(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["additionalContext"], "quick");
}

#[test]
fn the_dispatcher_runs_itself_through_the_filter_once_when_the_workspace_has_it() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    let seen = ws.path().join("filter-seen");
    hook(
        ws.path(),
        "h/x",
        &format!("cat > /dev/null; touch '{}'", marker.display()),
    );
    let filter = vec!["tools/dedup/dedup".to_string(), "run".to_string()];
    let d = plant(ws.path(), &[entry("Stop", "*", "h/x", &[])], &filter);

    // The control: without the program the hooks run and nothing went through it.
    let out = dispatch(&d, &["Stop"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(marker.exists());
    assert!(!seen.exists());
    fs::remove_file(&marker).unwrap();

    hook(
        ws.path(),
        "tools/dedup/dedup",
        &format!(
            "printf '%s\\n' \"$1\" >> '{}'\nshift\nexec \"$@\"",
            seen.display()
        ),
    );
    let out = dispatch(&d, &["Stop"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(marker.exists(), "the hook did not run through the filter");
    assert_eq!(
        fs::read_to_string(&seen).unwrap(),
        "run\n",
        "not once through the filter"
    );
}

#[test]
fn without_jq_the_dispatcher_says_so_and_exits_one() {
    let ws = tempfile::tempdir().unwrap();
    hook(ws.path(), "h/x", "cat > /dev/null");
    let d = plant(ws.path(), &[entry("Stop", "*", "h/x", &[])], &[]);
    let bare = crate::cmd::gates::tests::a_path_without_jq(ws.path());
    let out = dispatch(&d, &["Stop"], "{}", None, &[("PATH", Some(&bare))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("jq is not on PATH"),
        "{}",
        text(&out.stderr)
    );
    // the control: with jq it runs and says nothing
    let out = dispatch(&d, &["Stop"], "{}", None, &[]);
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "{}",
        text(&out.stderr)
    );
}
