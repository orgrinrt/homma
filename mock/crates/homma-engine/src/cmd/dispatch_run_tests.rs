//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The dispatcher as a script that runs: which entries it runs, how, for how
//! long and through what. What it runs is a property of bash and `jq`, and both
//! are here, so these run what homma writes rather than reading its text.

use std::fs;
use std::path::Path;

use super::tests::{dispatch, entry, hook, plant, text};

#[test]
fn a_hook_not_in_this_clone_is_passed_over_without_a_word() {
    let ws = tempfile::tempdir().unwrap();
    let d = plant(
        ws.path(),
        &[
            entry("PreToolUse", "*", "arvo/.claude/hooks/gone.sh", &["arvo"]),
            entry("PreToolUse", "*", "arvo/.claude/hooks/gone.sh", &[]),
            entry("Stop", "*", "arvo/.claude/hooks/gone.sh", &[]),
        ],
        &[],
    );
    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": ws.path().join("arvo/x").display().to_string() },
    })
    .to_string();
    for args in [&["PreToolUse", "sync"][..], &["Stop", "one", "2"][..]] {
        let out = dispatch(&d, args, &payload, None, &[]);
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "{args:?}: {}{}",
            text(&out.stdout),
            text(&out.stderr)
        );
    }
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
        ("stop", "Stop", false),
        ("stop2", "Stop", false),
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
    assert_eq!(ran(&["PreToolUse", "sync"]), vec!["pre"]);
    assert_eq!(ran(&["PostToolUse", "sync"]), vec!["post"]);
    assert_eq!(ran(&["PostToolUse", "async"]), vec!["late"]);
    assert_eq!(ran(&["Stop", "one", "4"]), vec!["stop2"]);
    // an entry named for another event is not run under this one
    assert!(ran(&["Stop", "one", "0"]).is_empty());
}

#[test]
fn an_event_that_does_not_fold_is_refused_together_and_named() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    hook(
        ws.path(),
        "h/x",
        &format!("cat > /dev/null; touch '{}'", marker.display()),
    );
    let d = plant(ws.path(), &[entry("Stop", "*", "h/x", &[])], &[]);
    let out = dispatch(&d, &["Stop", "sync"], "{}", None, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("not an event whose hooks are folded"));
    assert!(!marker.exists());
}

#[test]
fn an_entry_run_alone_hands_its_answer_on_as_it_gave_it() {
    let ws = tempfile::tempdir().unwrap();
    hook(
        ws.path(),
        "h/x",
        "cat > /dev/null; echo 'keep going'; echo 'because' >&2; exit 2",
    );
    hook(ws.path(), "h/y", "cat > /dev/null; echo plain context");
    let d = plant(
        ws.path(),
        &[entry("Stop", "*", "h/x", &[]), entry("SessionStart", "*", "h/y", &[])],
        &[],
    );
    let out = dispatch(&d, &["Stop", "one", "0"], "{}", None, &[]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(text(&out.stdout), "keep going\n");
    assert_eq!(text(&out.stderr), "because\n");
    let out = dispatch(&d, &["SessionStart", "one", "1"], "{}", None, &[]);
    assert!(out.status.success());
    assert_eq!(text(&out.stdout), "plain context\n");
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
        let out = dispatch(&d, &["PreToolUse", "sync"], &payload, None, &[]);
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
    e.args = "--strict 'two words' \"$UNSET_IN_THIS_TEST\"".into();
    let d = plant(ws.path(), &[e.clone()], &[]);
    let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    // an unset variable is read as nothing, as the host's shell read it
    assert_eq!(fs::read_to_string(&marker).unwrap(), "--strict|two words||");

    // The control: the same file with no program is not run, since it is not
    // executable.
    fs::remove_file(&marker).unwrap();
    e.runner = String::new();
    let d = plant(ws.path(), &[e], &[]);
    let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
    assert!(out.status.success());
    assert!(
        !marker.exists(),
        "a file that is not executable ran with no program"
    );
}

#[test]
fn arguments_that_are_not_words_are_named_and_the_other_hooks_still_run() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    hook(ws.path(), "h/bad", "cat > /dev/null");
    hook(
        ws.path(),
        "h/deny",
        r#"cat > /dev/null; echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"no"}}'"#,
    );
    hook(
        ws.path(),
        "h/x",
        &format!("cat > /dev/null; touch '{}'", marker.display()),
    );
    let mut bad = entry("PreToolUse", "*", "h/bad", &[]);
    bad.args = "'unbalanced".into();
    let d = plant(
        ws.path(),
        &[
            bad,
            entry("PreToolUse", "*", "h/deny", &[]),
            entry("PreToolUse", "*", "h/x", &[]),
        ],
        &[],
    );
    let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(marker.exists(), "a hook after the broken one did not run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        v["systemMessage"]
            .as_str()
            .unwrap()
            .contains("h/bad: exited 1: its arguments are not words"),
        "{v}"
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
            "cat > /dev/null; touch '{}'; for _ in $(seq 1 50); do [ -e '{}' ] && exit 0; sleep \
             0.1; done; exit 1",
            mine.display(),
            theirs.display()
        )
    };
    hook(ws.path(), "h/a", &wait_for(&a, &b));
    hook(ws.path(), "h/b", &wait_for(&b, &a));
    let d = plant(
        ws.path(),
        &[entry("PostToolUse", "*", "h/a", &[]), entry("PostToolUse", "*", "h/b", &[])],
        &[],
    );
    let out = dispatch(&d, &["PostToolUse", "sync"], "{}", None, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        out.stdout.is_empty() && out.stderr.is_empty(),
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
    // Without a `timeout` program on the PATH, as on a machine that has none:
    // the dispatcher keeps the time itself.
    let bin = ws.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    for program in ["bash", "jq", "cat", "dirname", "mktemp", "rm", "sleep", "grep"] {
        let found = std::process::Command::new("sh")
            .args(["-c", &format!("command -v {program}")])
            .output()
            .unwrap();
        let found = text(&found.stdout).trim().to_string();
        std::os::unix::fs::symlink(&found, bin.join(program)).unwrap();
    }
    let path = bin.to_string_lossy().into_owned();
    let started = std::time::Instant::now();
    let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[(
        "PATH",
        Some(&path),
    )]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the slow hook was waited on"
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["additionalContext"], "quick");
    assert!(
        v["systemMessage"]
            .as_str()
            .unwrap()
            .contains("h/slow: ran past its 1 seconds and was stopped"),
        "{v}"
    );
}

#[test]
fn a_hook_that_exits_124_itself_is_not_said_to_have_run_out_of_time() {
    let ws = tempfile::tempdir().unwrap();
    hook(ws.path(), "h/x", "cat > /dev/null; exit 124");
    let d = plant(ws.path(), &[entry("PreToolUse", "*", "h/x", &[])], &[]);
    let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
    assert_eq!(out.status.code(), Some(124));
    assert!(
        !text(&out.stderr).contains("ran past"),
        "{}",
        text(&out.stderr)
    );
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
    let d = plant(ws.path(), &[entry("PostToolUse", "*", "h/x", &[])], &filter);

    // The control: without the program the hooks run and nothing went through it.
    let out = dispatch(&d, &["PostToolUse", "sync"], "{}", None, &[]);
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
    let out = dispatch(&d, &["PostToolUse", "sync"], "{}", None, &[]);
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
    let d = plant(ws.path(), &[entry("PostToolUse", "*", "h/x", &[])], &[]);
    let bare = crate::cmd::gates::tests::a_path_without_jq(ws.path());
    let out = dispatch(&d, &["PostToolUse", "sync"], "{}", None, &[(
        "PATH",
        Some(&bare),
    )]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("jq is not on PATH"),
        "{}",
        text(&out.stderr)
    );
    // the control: with jq it runs and says nothing
    let out = dispatch(&d, &["PostToolUse", "sync"], "{}", None, &[]);
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "{}",
        text(&out.stderr)
    );
}
