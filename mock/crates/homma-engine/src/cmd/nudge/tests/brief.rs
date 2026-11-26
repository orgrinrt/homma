//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A reminder that says the brief of a prompt.

use super::*;

const BRIEF: &str = "[[nudges]]\nname = \"brief\"\nevent = \"UserPromptSubmit\"\nsays = \"What bears \
                     on this work:\"\nstrength = \"inform\"\nbrief = true\nttl = \"1h\"\n";

impl Ws {
    fn prompt(&self, now: u64, session: &str, text: &str) -> Option<Value> {
        let payload = json!({"session_id": session, "prompt": text});
        self.ask_at(now, "UserPromptSubmit", &payload.to_string())
    }
}

#[test]
fn a_prompt_is_briefed_after_the_rows_says() {
    let ws = Ws::new(BRIEF);
    let said = context(&ws.prompt(0, "s1", "write the parser's tests"));
    assert_eq!(
        said,
        "What bears on this work:\nrule r: brief of write the parser's tests"
    );
    assert_eq!(*ws.briefed.borrow(), ["write the parser's tests"]);
}

#[test]
fn the_same_prompt_is_briefed_once_until_the_ttl_passes() {
    let ws = Ws::new(BRIEF);
    context(&ws.prompt(0, "s1", "one work"));
    assert_eq!(ws.prompt(10, "s1", "one work"), None);
    // another prompt is another work, and is briefed
    context(&ws.prompt(20, "s1", "another work"));
    // the first again, now not the last briefed, is briefed again
    context(&ws.prompt(30, "s1", "one work"));
    // and the same text once the ttl has passed
    context(&ws.prompt(4000, "s1", "one work"));
    assert_eq!(ws.briefed.borrow().len(), 4);
}

#[test]
fn a_session_keeps_its_own_briefed_text() {
    let ws = Ws::new(BRIEF);
    context(&ws.prompt(0, "s1", "one work"));
    context(&ws.prompt(1, "s2", "one work"));
    assert_eq!(ws.prompt(2, "s1", "one work"), None);
}

#[test]
fn an_empty_prompt_or_an_empty_brief_says_nothing() {
    let ws = Ws::new(BRIEF);
    assert_eq!(ws.prompt(0, "s1", "   "), None);
    assert!(
        ws.briefed.borrow().is_empty(),
        "an empty text is not briefed"
    );
    assert_eq!(ws.prompt(1, "s1", "about nothing"), None);
    assert_eq!(ws.briefed.borrow().len(), 1);
}

#[test]
fn a_row_without_a_brief_asks_for_none() {
    let plain = "[[nudges]]\nname = \"hi\"\nevent = \"UserPromptSubmit\"\nsays = \"hello\"\nstrength \
                 = \"inform\"\n";
    let ws = Ws::new(plain);
    assert_eq!(context(&ws.prompt(0, "s1", "one work")), "hello");
    assert!(ws.briefed.borrow().is_empty());
}

#[test]
fn matches_reads_the_prompt_on_an_event_with_no_tool() {
    let rows = format!("{BRIEF}matches = 'tests'\n");
    let ws = Ws::new(&rows);
    assert_eq!(ws.prompt(0, "s1", "write the docs"), None);
    context(&ws.prompt(1, "s1", "write the tests"));
    assert_eq!(*ws.briefed.borrow(), ["write the tests"]);
}
