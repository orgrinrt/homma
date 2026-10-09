//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! An answer whose question was in a cut write: a tool result naming a call no
//! earlier line holds.

use serde_json::{Value, json};

use super::*;
use crate::cmd::capture::ask::Unasked;
use crate::cmd::capture::transcript::{Event, Happened, read};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

/// The start of the assistant line that put round `r` to the person, cut inside
/// a string in the question's options, as a cut write leaves it.
const ASKED_THEN_CUT: &str = r#"{"parentUuid":"p0","isSidechain":false,"message":{"role":"assistant","content":[{"type":"tool_use","id":"ask-r","name":"AskUserQuestion","input":{"questions":[{"question":"Which way?","header":"Way","multiSelect":false,"options":[{"label":"Le"#;

/// A whole line the person typed, which the cut write's neighbour is.
fn whole(uuid: &str) -> String {
    typed(uuid, T1, "typed while the question stood").to_string()
}

fn events(text: &str) -> anyhow::Result<Vec<Event>> {
    read(text).map(|t| t.events)
}

/// The rounds among `events`, whole, since the line they sit on is not what
/// is compared.
fn answered(events: &[Event]) -> Vec<&Happened> {
    events
        .iter()
        .map(|e| &e.what)
        .filter(|w| matches!(w, Happened::Answered(_)))
        .collect()
}

/// A result of round `r` with its call missing, carrying `result`.
fn result_of_r(result: Value) -> String {
    json!({"type": "user", "uuid": "r", "timestamp": T2,
           "message": {"content": [{"type": "tool_result", "tool_use_id": "ask-r"}]},
           "toolUseResult": result})
    .to_string()
}

/// Line 3 of a transcript whose line 2 is a cut write and a whole line.
fn with_the_question_cut(line_3: &str) -> String {
    format!(
        "{}\n{ASKED_THEN_CUT}{}\n{line_3}\n{}\n",
        typed("a", T0, "first"),
        whole("b"),
        typed("c", "2026-09-24T14:03:00.000Z", "last"),
    )
}

#[test]
fn an_answer_whose_question_was_in_a_cut_write_is_read_by_its_shape() {
    let answer = round("r", T2, "Left", "Red", Some("only if it is quick"));
    let t = read(&with_the_question_cut(&answer.to_string())).expect("reads");
    // The answer is in the capture, word for word the one the call would have
    // had.
    let whole_one = lines(&[
        typed("a", T0, "first"),
        asking("r"),
        answer.clone(),
        typed("c", "2026-09-24T14:03:00.000Z", "last"),
    ]);
    let control = read(&whole_one).expect("reads");
    assert_eq!(answered(&t.events).len(), 1, "{:?}", t.events);
    assert_eq!(answered(&t.events), answered(&control.events));
    // And it is said, naming the line.
    assert_eq!(t.unasked, [Unasked {
        line: 3,
        read: true,
    }]);
    // The control: with its call on a line of its own, nothing is said.
    assert!(control.unasked.is_empty());
}

#[test]
fn an_answer_that_cannot_be_read_is_said_not_refused() {
    // It has `questions` and cannot be read as a round: a question with no
    // `header`. The run goes on, and says which line.
    let broken = json!({"questions": [{"question": "q", "options": []}], "answers": {"q": "x"}});
    let t = read(&with_the_question_cut(&result_of_r(broken.clone()))).expect("not refused");
    assert!(answered(&t.events).is_empty(), "{:?}", t.events);
    assert_eq!(t.unasked, [Unasked {
        line: 3,
        read: false,
    }]);
    // `answers` alone is the same.
    let alone = json!({"answers": {"q": "x"}});
    let t = read(&with_the_question_cut(&result_of_r(alone))).expect("not refused");
    assert_eq!(t.unasked, [Unasked {
        line: 3,
        read: false,
    }]);
    // The control: the same broken result answering a call the transcript
    // holds is refused by its number, as it always was.
    let known = format!(
        "{}\n{}\n{}\n",
        typed("a", T0, "first"),
        asking("r"),
        result_of_r(broken),
    );
    let err = format!("{:#}", events(&known).expect_err("refused"));
    assert!(
        err.contains("transcript line 3") && err.contains("no `header`"),
        "{err}"
    );
}

#[test]
fn a_result_of_an_unknown_call_that_is_not_a_round_passes_without_a_word() {
    // The results the real session holds with no call on any line: the launch
    // of a sub-agent, and the other things a tool returns.
    let results = [
        json!({"agentId": "a1", "status": "async_launched", "prompt": "do it"}),
        json!("User rejected tool use"),
        json!({"stdout": "ok", "stderr": ""}),
        json!({"collection": "x", "rows": []}),
    ];
    for result in results {
        let t = read(&with_the_question_cut(&result_of_r(result.clone()))).expect("reads");
        assert!(answered(&t.events).is_empty(), "{result}");
        assert!(t.unasked.is_empty(), "{result}: {:?}", t.unasked);
    }
}

#[test]
fn an_unknown_call_is_said_on_the_note_whichever_way_it_went() {
    assert_eq!(
        Unasked {
            line: 39105,
            read: true,
        }
        .to_string(),
        "transcript line 39105 carries the result of a question the transcript does not hold, \
         read as a round by its own shape"
    );
    assert_eq!(
        Unasked {
            line: 7,
            read: false,
        }
        .to_string(),
        "transcript line 7 carries the result of a question the transcript does not hold, and it \
         could not be read as a round, so what it says is not in the capture"
    );
}
