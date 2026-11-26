//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The two shapes a marker counts in, and the sessions kept apart.

use super::*;

impl Ws {
    fn shell_as(&self, session: &str, now: u64, command: &str) -> Option<Value> {
        let payload = json!({
            "session_id": session,
            "tool_name": "Bash",
            "tool_input": {"command": command},
        });
        self.ask_at(now, "PreToolUse", &payload.to_string())
    }
}

/// A workspace whose grep row has stopped `grep -r foo .`.
fn stopped() -> Ws {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo ."));
    ws
}

#[test]
fn a_second_line_after_the_restated_call_is_no_answer() {
    let ws = stopped();
    denied(&ws.shell(10, "grep -r foo . # searched-first why\ngrep -r secret ."));
    assert!(ws.logged("answered").is_empty());
    // the stop stands, so the stopped call is still stopped
    denied(&ws.shell(20, "grep -r foo ."));
}

#[test]
fn a_command_after_the_marker_alone_is_no_answer() {
    let ws = stopped();
    denied(&ws.shell(10, ": searched-first why; grep -r secret ."));
    assert!(ws.logged("answered").is_empty());
    denied(&ws.shell(20, "grep -r foo ."));
}

#[test]
fn the_marker_alone_with_anything_that_runs_more_answers_nothing() {
    for joined in [
        ": searched-first why && grep -r secret .",
        ": searched-first why || grep -r secret .",
        ": searched-first why | grep -r secret .",
        ": searched-first why & grep -r secret .",
        ": searched-first why\ngrep -r secret .",
        ": searched-first $(grep -r secret .)",
        ": searched-first `grep -r secret .`",
        ": searched-first why > out",
        ": searched-first why < in",
    ] {
        let ws = stopped();
        let _ = ws.shell(10, joined);
        assert!(ws.logged("answered").is_empty(), "{joined:?}");
        denied(&ws.shell(20, "grep -r foo ."));
    }
    // the marker alone is the answer, which is the control
    let ws = stopped();
    assert_eq!(ws.shell(10, ": searched-first why"), None);
    assert_eq!(ws.shell(20, "grep -r foo ."), None);
}

#[test]
fn a_different_command_before_the_marker_is_no_answer() {
    let ws = stopped();
    denied(&ws.shell(10, "grep -r secret . # searched-first why"));
    assert!(ws.logged("answered").is_empty());
    denied(&ws.shell(20, "grep -r foo ."));
}

#[test]
fn the_marker_counts_only_on_the_last_line() {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo .\necho done"));
    denied(&ws.shell(10, "grep -r foo . # searched-first why\necho done"));
    assert!(ws.logged("answered").is_empty());
    // the stopped lines with the comment on the last one pass, which is the control
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo .\necho done"));
    assert_eq!(
        ws.shell(20, "grep -r foo .\necho done # searched-first why"),
        None
    );
    assert_eq!(ws.logged("answered")[0]["reason"], "why");
}

#[test]
fn a_session_answers_only_its_own_stop() {
    let ws = Ws::new(GREP);
    denied(&ws.shell_as("one", 0, "grep -r foo ."));
    // another session has nothing stopped, so its marker answers nothing
    assert_eq!(ws.shell_as("two", 10, ": searched-first why"), None);
    assert!(ws.logged("answered").is_empty());
    denied(&ws.shell_as("one", 20, "grep -r foo ."));

    // an answer taken in one session lets only that session's call through
    assert_eq!(ws.shell_as("one", 30, ": searched-first why"), None);
    assert_eq!(ws.shell_as("one", 40, "grep -r foo ."), None);
    denied(&ws.shell_as("two", 50, "grep -r foo ."));
}
