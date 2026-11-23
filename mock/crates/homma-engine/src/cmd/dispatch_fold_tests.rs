//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How the dispatcher folds several hooks' answers at a tool event into the one
//! the host reads, the way the host folds several hooks itself, and hands a lone
//! answer on untouched. Each runs real hooks that answer as a hook does.

use std::path::Path;

use super::tests::{dispatch, entry, hook, plant, text};

/// Plant one entry per `(name, body)` at `event`, every tool, and dispatch a
/// Bash call to them.
fn answers(ws: &Path, event: &str, hooks: &[(&str, &str)]) -> std::process::Output {
    let mut entries = Vec::new();
    for (name, body) in hooks {
        let run = format!("h/{name}");
        hook(ws, &run, &format!("cat > /dev/null\n{body}"));
        entries.push(entry(event, "*", &run, &[]));
    }
    let d = plant(ws, &entries, &[]);
    dispatch(&d, &[event, "sync"], r#"{"tool_name":"Bash"}"#, None, &[])
}

fn json(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): out={} err={}",
            text(&out.stdout),
            text(&out.stderr)
        )
    })
}

fn decision(d: &str, reason: &str) -> String {
    format!(
        r#"echo '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"{d}","permissionDecisionReason":"{reason}"}}}}'"#
    )
}

fn context(event: &str, text: &str) -> String {
    format!(
        r#"echo '{{"hookSpecificOutput":{{"hookEventName":"{event}","additionalContext":"{text}"}}}}'"#
    )
}

#[test]
fn a_lone_answer_is_handed_on_untouched_whatever_it_is() {
    for (body, rc, out, err) in [
        ("echo 'not json at all'", 0, "not json at all\n", ""),
        ("echo 'no' >&2; exit 2", 2, "", "no\n"),
        ("echo 'broke' >&2; exit 3", 3, "", "broke\n"),
        (
            "echo '{\"hookSpecificOutput\":\"oops\"}'",
            0,
            "{\"hookSpecificOutput\":\"oops\"}\n",
            "",
        ),
    ] {
        let ws = tempfile::tempdir().unwrap();
        let o = answers(ws.path(), "PreToolUse", &[("a", body), ("quiet", "true")]);
        assert_eq!(o.status.code(), Some(rc), "{body}");
        assert_eq!(text(&o.stdout), out, "{body}");
        assert_eq!(text(&o.stderr), err, "{body}");
    }
}

#[test]
fn a_deny_beats_an_ask_beats_an_allow_and_carries_its_own_reasons() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &decision("allow", "fine")),
        ("b", &decision("deny", "no")),
        ("c", &decision("ask", "maybe")),
        ("d", &decision("deny", "never")),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let h = &json(&out)["hookSpecificOutput"];
    assert_eq!(h["permissionDecision"], "deny");
    assert_eq!(h["permissionDecisionReason"], "no\n\nnever");
    assert_eq!(h["hookEventName"], "PreToolUse");

    // the control: without the denies the ask wins
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &decision("allow", "fine")),
        ("c", &decision("ask", "maybe")),
    ]);
    assert_eq!(
        json(&out)["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
}

#[test]
fn every_hooks_context_reaches_the_session() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &context("PreToolUse", "one")),
        ("b", "true"),
        ("c", &context("PreToolUse", "two")),
    ]);
    assert!(out.status.success());
    let ctx = json(&out)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(ctx.contains("one") && ctx.contains("two"), "{ctx}");
    assert!(
        json(&out)["hookSpecificOutput"]
            .get("permissionDecision")
            .is_none()
    );
}

#[test]
fn an_exit_two_before_the_call_is_a_deny_beside_the_others_answers() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &context("PreToolUse", "remember this")),
        ("b", "echo 'blocked: not here' >&2; exit 2"),
        ("c", &decision("allow", "fine")),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let h = &json(&out)["hookSpecificOutput"];
    assert_eq!(h["permissionDecision"], "deny");
    assert_eq!(h["permissionDecisionReason"], "blocked: not here");
    assert_eq!(h["additionalContext"], "remember this");
}

#[test]
fn an_exit_two_after_the_call_is_a_block_with_its_stderr_the_reason() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PostToolUse", &[
        ("a", "echo 'the output was cut' >&2; exit 2"),
        ("b", &context("PostToolUse", "noted")),
        (
            "c",
            r#"echo '{"decision":"block","reason":"and it failed"}'"#,
        ),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = json(&out);
    assert_eq!(v["decision"], "block");
    assert_eq!(v["reason"], "the output was cut\n\nand it failed");
    assert_eq!(v["hookSpecificOutput"]["additionalContext"], "noted");
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PostToolUse");
}

#[test]
fn continue_and_messages_fold_across_hooks() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PostToolUse", &[
        (
            "a",
            r#"echo '{"continue":false,"stopReason":"out of budget"}'"#,
        ),
        ("b", r#"echo '{"continue":true,"systemMessage":"one"}'"#),
        (
            "c",
            r#"echo '{"systemMessage":"two","suppressOutput":true}'"#,
        ),
    ]);
    let v = json(&out);
    assert_eq!(v["continue"], false);
    assert_eq!(v["stopReason"], "out of budget");
    assert_eq!(v["systemMessage"], "one\n\ntwo");
    assert_eq!(v["suppressOutput"], true);
    assert!(v.get("hookSpecificOutput").is_none(), "{v}");
}

#[test]
fn a_malformed_answer_beside_a_deny_is_named_and_the_deny_stands() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("bad", r#"echo '{"hookSpecificOutput":"oops"}'"#),
        ("two", r#"echo '{"a":1}{"b":2}'"#),
        ("deny", &decision("deny", "no")),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = json(&out);
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    let m = v["systemMessage"].as_str().unwrap();
    assert!(
        m.contains("h/bad: answered JSON the host does not read"),
        "{m}"
    );
    assert!(
        m.contains("h/two: answered JSON the host does not read"),
        "{m}"
    );
}

#[test]
fn a_hook_that_fails_another_way_is_named_to_the_person_and_the_others_still_answer() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("broken", "echo 'it broke' >&2; exit 3"),
        ("fine", &context("PreToolUse", "still here")),
    ]);
    assert!(out.status.success());
    let v = json(&out);
    assert_eq!(v["systemMessage"], "h/broken: exited 3: it broke");
    assert_eq!(v["hookSpecificOutput"]["additionalContext"], "still here");
}

#[test]
fn plain_text_beside_json_at_a_tool_event_goes_to_stderr() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("plain", "echo 'a line for the transcript'"),
        ("json", &context("PreToolUse", "for the session")),
    ]);
    assert!(out.status.success());
    assert_eq!(
        json(&out)["hookSpecificOutput"]["additionalContext"],
        "for the session"
    );
    assert!(text(&out.stderr).contains("a line for the transcript"));
}

#[test]
fn a_large_answer_folds_like_a_small_one() {
    let ws = tempfile::tempdir().unwrap();
    // Three hundred kilobytes, past what one argument to a program may carry.
    let out = answers(ws.path(), "PreToolUse", &[
        ("big", "head -c 300000 /dev/zero | tr '\\0' x; echo"),
        ("deny", &decision("deny", "no")),
        ("err", "head -c 300000 /dev/zero | tr '\\0' y >&2; exit 2"),
    ]);
    assert!(
        out.status.success(),
        "{}",
        &text(&out.stderr)[.. 200.min(out.stderr.len())]
    );
    let h = &json(&out)["hookSpecificOutput"];
    assert_eq!(h["permissionDecision"], "deny");
    assert!(h["permissionDecisionReason"].as_str().unwrap().len() > 300_000);
}
