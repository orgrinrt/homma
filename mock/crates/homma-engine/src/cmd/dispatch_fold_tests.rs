//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How the dispatcher folds several hooks' answers into the one the host reads,
//! the way the host folds several hooks itself. Each runs real hooks that answer
//! as a hook does.

use std::path::Path;

use super::tests::{dispatch, entry, hook, plant};

/// Plant one entry per `(name, body)` at `event`, every tool, and dispatch an
/// empty call.
fn answers(ws: &Path, event: &str, hooks: &[(&str, &str)]) -> std::process::Output {
    let mut entries = Vec::new();
    for (name, body) in hooks {
        let run = format!("h/{name}");
        hook(ws, &run, &format!("cat > /dev/null\n{body}"));
        entries.push(entry(event, "*", &run, &[]));
    }
    let d = plant(ws, &entries, &[]);
    dispatch(&d, &[event], r#"{"tool_name":"Bash"}"#, None, &[])
}

fn json(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): out={} err={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn decision(d: &str, reason: &str) -> String {
    format!(
        r#"echo '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"{d}","permissionDecisionReason":"{reason}"}}}}'"#
    )
}

fn context(text: &str) -> String {
    format!(
        r#"echo '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","additionalContext":"{text}"}}}}'"#
    )
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
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let h = &json(&out)["hookSpecificOutput"];
    assert_eq!(h["permissionDecision"], "deny");
    assert_eq!(h["permissionDecisionReason"], "no\n\nnever");
    assert_eq!(h["hookEventName"], "PreToolUse");

    // the controls: without the denies the ask wins, and alone an allow stands
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &decision("allow", "fine")),
        ("c", &decision("ask", "maybe")),
    ]);
    assert_eq!(
        json(&out)["hookSpecificOutput"]["permissionDecision"],
        "ask"
    );
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[(
        "a",
        &decision("allow", "fine"),
    )]);
    assert_eq!(
        json(&out)["hookSpecificOutput"]["permissionDecision"],
        "allow"
    );
}

#[test]
fn every_hooks_context_reaches_the_session() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &context("one")),
        ("b", "true"),
        ("c", &context("two")),
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
fn an_exit_two_blocks_and_carries_every_other_hooks_context() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("a", &context("remember this")),
        ("b", "echo 'blocked: not here' >&2; exit 2"),
        ("c", &decision("allow", "fine")),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("blocked: not here"), "{err}");
    assert!(err.contains("remember this"), "{err}");
}

#[test]
fn a_block_continue_and_messages_fold_across_hooks() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "Stop", &[
        (
            "a",
            r#"echo '{"decision":"block","reason":"finish the tests"}'"#,
        ),
        ("b", r#"echo '{"systemMessage":"one"}'"#),
        (
            "c",
            r#"echo '{"systemMessage":"two","suppressOutput":true}'"#,
        ),
        ("d", r#"echo '{"decision":"block","reason":"and push"}'"#),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = json(&out);
    assert_eq!(v["decision"], "block");
    assert_eq!(v["reason"], "finish the tests\n\nand push");
    assert_eq!(v["systemMessage"], "one\n\ntwo");
    assert_eq!(v["suppressOutput"], true);
    assert!(v.get("continue").is_none(), "{v}");
    assert!(v.get("hookSpecificOutput").is_none(), "{v}");

    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "Stop", &[
        (
            "a",
            r#"echo '{"continue":false,"stopReason":"out of budget"}'"#,
        ),
        ("b", r#"echo '{"continue":true}'"#),
    ]);
    let v = json(&out);
    assert_eq!(v["continue"], false);
    assert_eq!(v["stopReason"], "out of budget");
}

#[test]
fn plain_text_stays_text_alone_and_is_context_at_session_start() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "SessionStart", &[
        ("a", "echo hello"),
        ("b", "echo there"),
    ]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("hello") && text.contains("there"), "{text}");

    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "SessionStart", &[
        ("a", "echo hello"),
        (
            "b",
            r#"echo '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"json"}}'"#,
        ),
    ]);
    let ctx = json(&out)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(ctx.contains("hello") && ctx.contains("json"), "{ctx}");
    assert_eq!(
        json(&out)["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
}

#[test]
fn a_hook_that_fails_another_way_is_named_and_the_others_still_answer() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[
        ("broken", "echo 'it broke' >&2; exit 3"),
        ("fine", &context("still here")),
    ]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("h/broken: exited 3") && err.contains("it broke"),
        "{err}"
    );
    assert_eq!(
        json(&out)["hookSpecificOutput"]["additionalContext"],
        "still here"
    );

    // alone, the failure is the answer: exit 1, said on stderr
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[("broken", "exit 3")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("h/broken: exited 3"));
}

#[test]
fn a_single_hooks_answer_passes_through_unchanged_in_meaning() {
    let ws = tempfile::tempdir().unwrap();
    let out = answers(ws.path(), "PreToolUse", &[(
        "a",
        r#"echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"command":"ls"}}}'"#,
    )]);
    let h = &json(&out)["hookSpecificOutput"];
    assert_eq!(h["permissionDecision"], "allow");
    assert_eq!(h["updatedInput"]["command"], "ls");
    assert!(h.get("permissionDecisionReason").is_none(), "{h}");
}
