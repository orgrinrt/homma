//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a nudge row does with a call: stop it, take its answer, refuse it, or
//! remind, and what it keeps and logs.

use std::path::Path;

use homma_api::{AgentTable, Nudges};
use serde_json::{Value, json};

use super::*;

/// The workspace a test runs in, its rows and the moment it asks at.
struct Ws {
    dir:     tempfile::TempDir,
    nudges:  Nudges,
    /// The texts a brief was asked for, in order.
    briefed: std::cell::RefCell<Vec<String>>,
}

impl Ws {
    fn new(rows: &str) -> Ws {
        Ws {
            dir:     tempfile::tempdir().unwrap(),
            nudges:  toml::from_str::<AgentTable>(rows).unwrap().nudges,
            briefed: std::cell::RefCell::default(),
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn store_dir(&self) -> std::path::PathBuf {
        self.root().join("state/homma/nudges")
    }

    fn ask_at(&self, now: u64, event: &str, payload: &str) -> Option<Value> {
        // a stand-in brief: none for a text about nothing, else one naming the text
        let brief = |text: &str| {
            self.briefed.borrow_mut().push(text.to_string());
            (!text.contains("nothing")).then(|| format!("rule r: brief of {text}"))
        };
        let ctx = Ctx {
            root: self.root(),
            store: Store::at(self.store_dir()),
            now,
            brief: &brief,
        };
        answer(&ctx, &self.nudges, event, payload).map(|s| serde_json::from_str(&s).unwrap())
    }

    fn ask(&self, now: u64, tool: &str, input: Value) -> Option<Value> {
        let payload = json!({"session_id": "s1", "tool_name": tool, "tool_input": input});
        self.ask_at(now, "PreToolUse", &payload.to_string())
    }

    fn shell(&self, now: u64, command: &str) -> Option<Value> {
        self.ask(now, "Bash", json!({"command": command}))
    }

    fn script(&self, rel: &str, body: &str) {
        homma_fixture::script(
            &self.root().join(rel),
            &format!("#!/usr/bin/env bash\n{body}\n"),
            0o755,
        );
    }

    fn log(&self) -> Vec<Value> {
        let text =
            std::fs::read_to_string(self.store_dir().with_extension("log")).unwrap_or_default();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn logged(&self, what: &str) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|e| e["what"] == what)
            .collect()
    }
}

fn denied(v: &Option<Value>) -> String {
    let out = v.as_ref().expect("the call was stopped");
    assert_eq!(
        out["hookSpecificOutput"]["permissionDecision"], "deny",
        "{out}"
    );
    out["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .to_string()
}

fn context(v: &Option<Value>) -> String {
    let out = v.as_ref().expect("something was said");
    out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_string()
}

const GREP: &str = "[[nudges]]\nname = \"grep\"\ntools = \"Bash|Grep\"\nmatches = '\\bgrep\\b'\nsays = \
                    \"search first\"\nstrength = \"once\"\nanswer = \"searched-first\"\nttl = \"10m\"\n";

const ASKED: &str = "[[nudges]]\nname = \"asked\"\ntools = \"AskUserQuestion\"\nsays = \"asked \
                     before\"\nstrength = \"once\"\nttl = \"2h\"\n";

#[test]
fn a_call_no_row_takes_gets_nothing_and_one_a_row_takes_is_stopped() {
    let ws = Ws::new(GREP);
    assert_eq!(ws.shell(0, "ls -la"), None);
    assert_eq!(
        ws.ask(0, "Read", json!({"file_path": "grep"})),
        None,
        "the tool is not taken"
    );
    let said = denied(&ws.shell(0, "grep -r foo ."));
    assert!(said.starts_with("search first"), "{said}");
    assert!(said.contains("# searched-first <why>"), "{said}");
    assert_eq!(ws.logged("stopped").len(), 1);
}

#[test]
fn an_event_with_no_rows_prints_nothing_and_reads_no_payload() {
    let ws = Ws::new(GREP);
    assert_eq!(ws.ask_at(0, "Stop", "not json"), None);
    assert!(ws.log().is_empty());
    // the same payload on an event with rows is logged as unreadable, which is the control
    assert_eq!(ws.ask_at(0, "PreToolUse", "not json"), None);
    assert_eq!(ws.logged("payload-unreadable").len(), 1);
}

#[test]
fn without_a_marker_the_same_call_made_again_is_the_answer() {
    let ws = Ws::new(ASKED);
    let q = json!({"questions": [{"question": "which?"}]});
    denied(&ws.ask(0, "AskUserQuestion", q.clone()));
    assert_eq!(ws.ask(5, "AskUserQuestion", q.clone()), None);
    assert_eq!(ws.logged("answered")[0]["reason"], "made again");
    // still answered later within the ttl
    assert_eq!(ws.ask(60, "AskUserQuestion", q.clone()), None);
    // another question is stopped, which is the control
    let other = json!({"questions": [{"question": "where?"}]});
    assert!(denied(&ws.ask(70, "AskUserQuestion", other)).contains("make the same call again"));
}

#[test]
fn without_a_marker_a_repeat_after_the_ttl_is_stopped_again() {
    let ws = Ws::new(ASKED);
    let q = json!({"questions": [{"question": "which?"}]});
    denied(&ws.ask(0, "AskUserQuestion", q.clone()));
    denied(&ws.ask(7200, "AskUserQuestion", q.clone()));
    assert!(ws.logged("answered").is_empty());
}

#[test]
fn the_stopped_command_carrying_the_marker_is_the_answer_and_runs_at_once() {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo ."));
    assert_eq!(
        ws.shell(
            10,
            "grep -r foo . # searched-first the search found no symbol"
        ),
        None
    );
    let taken = ws.logged("answered");
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0]["reason"], "the search found no symbol");
    assert_eq!(taken[0]["call"], "grep -r foo .");
    // made again plain within the ttl it passes too
    assert_eq!(ws.shell(20, "grep -r foo ."), None);
    // a different grep is stopped, which is the control
    denied(&ws.shell(30, "grep -r bar ."));
}

#[test]
fn the_marker_alone_answers_and_the_stopped_call_then_passes() {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo ."));
    assert_eq!(
        ws.shell(10, ": searched-first the index has no text search"),
        None
    );
    assert_eq!(ws.shell(20, "grep -r foo ."), None);
    assert_eq!(
        ws.logged("answered")[0]["reason"],
        "the index has no text search"
    );
}

#[test]
fn a_marker_with_no_stop_to_answer_answers_nothing() {
    let ws = Ws::new(GREP);
    assert_eq!(ws.shell(0, ": searched-first nothing was stopped"), None);
    assert!(ws.logged("answered").is_empty());
    denied(&ws.shell(10, "grep -r foo ."));
    // a marker on a call made before any stop does not pass it either
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo . # searched-first early"));
}

#[test]
fn an_answer_waits_no_longer_than_the_ttl() {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep -r foo ."));
    // ten minutes later the stop is gone, so the call carrying the marker is a new stop
    denied(&ws.shell(600, "grep -r foo . # searched-first late"));
    assert!(ws.logged("answered").is_empty());
}

#[test]
fn a_stopped_call_of_another_tool_passes_once_a_shell_call_answers_it() {
    let ws = Ws::new(GREP);
    let input = json!({"pattern": "grep", "path": "src"});
    denied(&ws.ask(0, "Grep", input.clone()));
    let said = denied(&ws.ask(1, "Grep", input.clone()));
    assert!(
        said.contains("answer with the shell command `: searched-first <why>`"),
        "{said}"
    );
    assert_eq!(ws.shell(2, ": searched-first symbols only"), None);
    assert_eq!(ws.ask(3, "Grep", input), None);
}

#[test]
fn evidence_that_fails_keeps_the_stop_and_says_why() {
    let ws = Ws::new(&format!("{GREP}evidence = \"scripts/searched\"\n"));
    let seen = ws.root().join("seen");
    ws.script(
        "scripts/searched",
        &format!("cat > '{}'\nprintf '%s' \"$HOMMA_NUDGE_REASON\" >> '{}'\necho 'no search ran in this repository'\nexit 1", seen.display(), seen.display()),
    );
    denied(&ws.shell(0, "grep -r foo ."));
    let said = denied(&ws.shell(10, "grep -r foo . # searched-first trust me"));
    assert!(said.contains("The answer was not taken"), "{said}");
    let seen_text = std::fs::read_to_string(&seen).unwrap();
    assert!(
        seen_text.contains("\"command\":\"grep -r foo .\""),
        "it reads the stopped call: {seen_text}"
    );
    assert!(seen_text.ends_with("trust me"), "{seen_text}");
    assert_eq!(ws.logged("evidence-refused").len(), 1);
    denied(&ws.shell(20, "grep -r foo ."));

    // evidence that holds takes the answer, which is the control
    ws.script("scripts/searched", "cat > /dev/null\nexit 0");
    assert_eq!(ws.shell(30, "grep -r foo . # searched-first ran it"), None);
}

#[test]
fn a_refusing_row_stops_every_time() {
    let ws = Ws::new(
        "[[nudges]]\nname = \"no-push\"\ntools = \"Bash\"\nmatches = 'git push \
                      --force'\nsays = \"never force\"\nstrength = \"refuse\"\n",
    );
    for t in [0, 1, 2] {
        assert_eq!(
            denied(&ws.shell(t, "git push --force origin x")),
            "never force"
        );
    }
    assert_eq!(ws.logged("refused").len(), 3);
    assert_eq!(ws.shell(3, "git push origin x"), None);
}

#[test]
fn a_reminder_is_said_once_per_ttl_on_any_event() {
    let ws = Ws::new(
        "[[nudges]]\nname = \"tokens\"\nevent = \"SessionStart\"\nsays = \"mind the \
                      tokens\"\nstrength = \"inform\"\nttl = \"1h\"\n",
    );
    let start = json!({"session_id": "s1"}).to_string();
    let out = ws.ask_at(0, "SessionStart", &start);
    assert_eq!(context(&out), "mind the tokens");
    assert_eq!(
        out.unwrap()["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    assert_eq!(ws.ask_at(100, "SessionStart", &start), None);
    assert_eq!(
        context(&ws.ask_at(3600, "SessionStart", &start)),
        "mind the tokens"
    );
    // another session has its own record, so it is told at once
    let other = json!({"session_id": "s2"}).to_string();
    assert_eq!(
        context(&ws.ask_at(100, "SessionStart", &other)),
        "mind the tokens"
    );
}

#[test]
fn due_reminders_are_one_notice_and_a_stop_says_none_of_them() {
    let rows = format!(
        "[[nudges]]\nname = \"one\"\nsays = \"remind one\"\nstrength = \"inform\"\n[[nudges]]\nname = \
         \"two\"\nsays = \"remind two\"\nstrength = \"inform\"\n{GREP}"
    );
    let ws = Ws::new(&rows);
    let said = denied(&ws.shell(0, "grep x"));
    assert!(!said.contains("remind"), "{said}");
    assert_eq!(
        context(&ws.shell(1, "ls")),
        "remind one\nremind two",
        "still due after the stop"
    );
    assert_eq!(ws.shell(2, "ls"), None);
}

#[test]
fn a_predicate_decides_whether_a_row_applies_and_adds_what_it_printed() {
    let ws = Ws::new(&format!("{ASKED}when = \"scripts/asked\"\n"));
    let q = json!({"questions": [{"question": "which?"}]});
    ws.script("scripts/asked", "cat > /dev/null\nexit 1");
    assert_eq!(ws.ask(0, "AskUserQuestion", q.clone()), None);
    ws.script(
        "scripts/asked",
        "grep -q which && echo 'answered on 3 Nov: the first' && exit 0; exit 1",
    );
    let said = denied(&ws.ask(1, "AskUserQuestion", q.clone()));
    assert!(
        said.starts_with("asked before\nanswered on 3 Nov: the first"),
        "{said}"
    );
}

#[test]
fn a_predicate_that_breaks_lets_the_call_pass_and_is_logged() {
    let ws = Ws::new(&format!("{ASKED}when = \"scripts/asked\"\n"));
    let q = json!({"questions": [{"question": "which?"}]});
    ws.script("scripts/asked", "cat > /dev/null\necho broken >&2\nexit 3");
    assert_eq!(ws.ask(0, "AskUserQuestion", q.clone()), None);
    let failed = ws.logged("predicate-failed");
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0]["reason"]
            .as_str()
            .unwrap()
            .contains("exit 3: broken"),
        "{failed:?}"
    );
    let missing = Ws::new(&format!("{ASKED}when = \"scripts/none\"\n"));
    assert_eq!(missing.ask(0, "AskUserQuestion", q), None);
    assert_eq!(missing.logged("predicate-failed").len(), 1);
}

#[test]
fn a_record_that_does_not_read_is_an_empty_one_and_a_log_line() {
    let ws = Ws::new(GREP);
    std::fs::create_dir_all(ws.store_dir()).unwrap();
    std::fs::write(ws.store_dir().join("s1.json"), "{ not json").unwrap();
    denied(&ws.shell(0, "grep x"));
    assert_eq!(ws.logged("record-unreadable").len(), 1);
    // and the stop it then made is kept, so the answer works, which is the control
    assert_eq!(ws.shell(1, "grep x # searched-first because"), None);
}

#[test]
fn a_record_that_cannot_be_kept_lets_the_verb_answer_and_says_so_in_the_log() {
    let ws = Ws::new(GREP);
    std::fs::create_dir_all(ws.store_dir()).unwrap();
    std::fs::create_dir_all(ws.store_dir().join("s1.json")).unwrap();
    denied(&ws.shell(0, "grep x"));
    assert!(!ws.logged("record-not-kept").is_empty());
}

#[test]
fn the_marker_is_found_only_as_a_word() {
    let ws = Ws::new(GREP);
    denied(&ws.shell(0, "grep x"));
    denied(&ws.shell(1, "grep x # not-searched-first why"));
    assert!(ws.logged("answered").is_empty());
}

mod brief;
mod marker;
mod rows;
