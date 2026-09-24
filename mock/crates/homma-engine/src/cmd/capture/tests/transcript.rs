//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use serde_json::json;

use super::*;
use crate::cmd::capture::transcript::{Answer, Happened, NOTES_ONLY, answered, read};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

fn said(e: &crate::cmd::capture::transcript::Event) -> &str {
    match &e.what {
        Happened::Said(w) => w,
        other => panic!("not said: {other:?}"),
    }
}

#[test]
fn a_typed_line_is_read_word_for_word() {
    let words = "  Keep it,\n\nand the \"quotes\" and `ticks` and trailing space \n";
    let events = read(&lines(&[typed("a", T0, words)])).expect("reads");
    assert_eq!(events.len(), 1);
    assert_eq!(said(&events[0]), words);
    assert_eq!(events[0].at.to_string(), "2026-09-24T14:00:00Z");
}

#[test]
fn only_a_human_origin_makes_a_line_the_persons() {
    // Every one of these is a typed line in the transcript and none is his:
    // hook feedback, a compaction summary, a task notification, command output,
    // and a prompt the harness sends itself.
    let not_his = [
        json!({"type": "user", "uuid": "1", "timestamp": T0, "isMeta": true,
               "message": {"content": "Stop hook feedback: keep going"}}),
        json!({"type": "user", "uuid": "2", "timestamp": T0, "isCompactSummary": true,
               "message": {"content": "This session is being continued"}}),
        json!({"type": "user", "uuid": "3", "timestamp": T0, "origin": {"kind": "task-notification"},
               "message": {"content": "<task-notification>done</task-notification>"}}),
        json!({"type": "user", "uuid": "4", "timestamp": T0,
               "message": {"content": "<local-command-stdout>ok</local-command-stdout>"}}),
        json!({"type": "user", "uuid": "5", "timestamp": T0,
               "message": {"content": "Review this change for security vulnerabilities."}}),
    ];
    assert!(read(&lines(&not_his)).expect("reads").is_empty());
}

#[test]
fn a_slash_command_and_a_pasted_path_are_his_when_he_typed_them() {
    // The control for the test above: the text filter it replaces would drop
    // both of these, and the origin keeps them.
    let events = read(&lines(&[
        typed("a", T0, "/compact \"keep the goal\""),
        typed("b", T1, "<not a tag> but his"),
        typed(
            "c",
            T2,
            "/Users/someone/doc.pdf this should be extractable?",
        ),
    ]))
    .expect("reads");
    let got: Vec<&str> = events.iter().map(said).collect();
    assert_eq!(got, [
        "/compact \"keep the goal\"",
        "<not a tag> but his",
        "/Users/someone/doc.pdf this should be extractable?",
    ]);
}

#[test]
fn words_typed_into_a_running_turn_are_read_off_the_attachment() {
    let queue = json!({"type": "queue-operation", "operation": "enqueue",
                       "timestamp": T0, "content": "while you work"});
    let removed = json!({"type": "queue-operation", "operation": "remove",
                         "timestamp": T1, "content": "while you work"});
    let events = read(&lines(&[queue, removed, queued("q", T1, "while you work")])).expect("reads");
    // Once, from the attachment, and never from the queue's own bookkeeping.
    assert_eq!(events.len(), 1);
    assert_eq!(said(&events[0]), "while you work");
}

#[test]
fn a_queued_command_that_is_not_his_is_passed_over() {
    let mut note = queued("q", T0, "<task-notification>x</task-notification>");
    note["attachment"]["origin"] = json!(null);
    note["attachment"]["commandMode"] = json!("task-notification");
    let mut peer = queued("p", T0, "from another session");
    peer["attachment"]["origin"] = json!({"kind": "peer", "from": "uds:/tmp/x"});
    assert!(read(&lines(&[note, peer])).expect("reads").is_empty());
}

#[test]
fn a_replayed_line_is_read_once() {
    let events = read(&lines(&[typed("a", T0, "once"), typed("a", T0, "once")])).expect("reads");
    assert_eq!(events.len(), 1);
}

#[test]
fn a_sub_agents_lines_are_not_this_conversation() {
    let mut side = typed("a", T0, "from inside an agent");
    side["isSidechain"] = json!(true);
    assert!(read(&lines(&[side])).expect("reads").is_empty());
}

#[test]
fn the_agents_last_text_before_an_event_rides_with_it() {
    let events = read(&lines(&[
        said_by_agent("x", T0, "first"),
        said_by_agent("y", T0, "the question"),
        typed("a", T1, "answer"),
        typed("b", T2, "another"),
    ]))
    .expect("reads");
    assert_eq!(events[0].before.as_deref(), Some("the question"));
    assert_eq!(events[1].before.as_deref(), Some("the question"));
}

#[test]
fn an_agent_line_with_no_text_keeps_the_text_before_it() {
    let tools_only = json!({"type": "assistant", "uuid": "z", "timestamp": T1,
        "message": {"content": [{"type": "tool_use", "id": "t", "name": "Read", "input": {}}]}});
    let events = read(&lines(&[
        said_by_agent("x", T0, "kept"),
        tools_only,
        typed("a", T2, "hi"),
    ]))
    .expect("reads");
    assert_eq!(events[0].before.as_deref(), Some("kept"));
}

#[test]
fn a_first_event_has_nothing_before_it() {
    let events = read(&lines(&[typed("a", T0, "hi")])).expect("reads");
    assert_eq!(events[0].before, None);
}

#[test]
fn text_typed_beside_an_image_keeps_the_text() {
    let line = json!({"type": "user", "uuid": "a", "timestamp": T0, "origin": {"kind": "human"},
    "message": {"content": [
        {"type": "image", "source": {}},
        {"type": "text", "text": "look at this"},
        {"type": "text", "text": "and this"},
    ]}});
    let events = read(&lines(&[line])).expect("reads");
    assert_eq!(said(&events[0]), "look at this\n\nand this");
}

#[test]
fn an_image_alone_is_no_words() {
    let line = json!({"type": "user", "uuid": "a", "timestamp": T0, "origin": {"kind": "human"},
        "message": {"content": [{"type": "image", "source": {}}]}});
    assert!(read(&lines(&[line])).expect("reads").is_empty());
}

#[test]
fn an_answered_round_is_read_whole_off_its_result() {
    let events = read(&lines(&[round(
        "r",
        T0,
        "Right",
        "Red, Blue",
        Some("  his note "),
    )]))
    .expect("reads");
    let Happened::Answered(qs) = &events[0].what else {
        panic!("not a round");
    };
    assert_eq!(qs.len(), 2);
    assert_eq!(qs[0].text, "Which way?");
    assert_eq!(qs[0].header, "Way");
    assert_eq!(qs[0].options[1].preview.as_deref(), Some("```\nR\n```"));
    assert_eq!(qs[0].options[0].preview, None);
    assert_eq!(qs[0].answer, Answer::Chose(vec!["Right".into()]));
    assert_eq!(qs[0].notes.as_deref(), Some("  his note "));
    assert_eq!(
        qs[1].answer,
        Answer::Chose(vec!["Red".into(), "Blue".into()])
    );
    assert_eq!(qs[1].notes, None);
}

#[test]
fn an_answer_is_a_choice_typed_words_or_nothing() {
    let options: Vec<_> = ["A", "B", "C, D"]
        .iter()
        .map(|l| {
            crate::cmd::capture::transcript::Choice {
                label:       l.to_string(),
                description: String::new(),
                preview:     None,
            }
        })
        .collect();
    assert_eq!(answered("A", &options), Answer::Chose(vec!["A".into()]));
    assert_eq!(
        answered("A, B", &options),
        Answer::Chose(vec!["A".into(), "B".into()])
    );
    // A label with the separator inside it is still one label.
    assert_eq!(
        answered("C, D", &options),
        Answer::Chose(vec!["C, D".into()])
    );
    assert_eq!(answered(NOTES_ONLY, &options), Answer::Nothing);
    // Anything that is not wholly labels is his.
    assert_eq!(
        answered("A, and more", &options),
        Answer::Typed("A, and more".into())
    );
    assert_eq!(answered("a", &options), Answer::Typed("a".into()));
    assert_eq!(answered("", &options), Answer::Typed(String::new()));
    assert_eq!(
        answered("just merge both now", &options),
        Answer::Typed("just merge both now".into())
    );
}

#[test]
fn an_unanswered_question_is_nothing_and_empty_notes_are_none() {
    let mut r = round("r", T0, "Left", "Red", Some(""));
    r["toolUseResult"]["answers"]
        .as_object_mut()
        .expect("a map")
        .remove("Which colours?");
    let events = read(&lines(&[r])).expect("reads");
    let Happened::Answered(qs) = &events[0].what else {
        panic!("not a round");
    };
    assert_eq!(qs[1].answer, Answer::Nothing);
    assert_eq!(qs[0].notes, None);
}

#[test]
fn a_tool_result_without_answers_is_not_a_round() {
    let line = json!({"type": "user", "uuid": "a", "timestamp": T0,
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]},
        "toolUseResult": {"stdout": "ok"}});
    assert!(read(&lines(&[line])).expect("reads").is_empty());
}

#[test]
fn a_malformed_line_of_a_read_shape_is_refused_by_number() {
    let cases = [
        ("not json", "line 2 is not JSON".to_string()),
        ("[1, 2]", "line 2 is not an object".to_string()),
        (
            &*json!({"type": "user", "uuid": "b", "origin": {"kind": "human"},
                     "message": {"content": "no stamp"}})
            .to_string(),
            "line 2".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": "yesterday",
                     "origin": {"kind": "human"}, "message": {"content": "x"}})
            .to_string(),
            "is not an instant".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1, "origin": {"kind": "human"}})
                .to_string(),
            "no `message.content`".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1, "origin": {"kind": "human"},
                     "message": {"content": 7}})
            .to_string(),
            "neither text nor blocks".to_string(),
        ),
        (
            &*json!({"type": "attachment", "uuid": "b", "timestamp": T1,
                     "attachment": {"type": "queued_command", "origin": {"kind": "human"}}})
            .to_string(),
            "no `prompt`".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": {}}})
            .to_string(),
            "no `questions`".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": [], "questions": []}})
            .to_string(),
            "`answers` is not a map".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": {}, "questions": [{"question": "q", "options": []}]}})
            .to_string(),
            "no `header`".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": {}, "questions": [{"question": "q", "header": "h"}]}})
            .to_string(),
            "has no `options`".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": {"q": 3}, "questions": [
                         {"question": "q", "header": "h", "options": [{"label": "l", "description": "d"}]}]}})
            .to_string(),
            "is not text".to_string(),
        ),
        (
            &*json!({"type": "user", "uuid": "b", "timestamp": T1,
                     "toolUseResult": {"answers": {}, "questions": [
                         {"question": "q", "header": "h", "options": [{"label": "l"}]}]}})
            .to_string(),
            "no `description`".to_string(),
        ),
    ];
    for (line, want) in cases {
        let text = format!("{}\n{line}\n", typed("a", T0, "fine"));
        let err = format!("{:#}", read(&text).expect_err(line));
        assert!(err.contains(&want), "{line}: {err}");
        assert!(err.contains("line 2"), "{line}: {err}");
    }
}

#[test]
fn lines_of_shapes_nobody_reads_pass_without_a_word() {
    let text = lines(&[
        json!({"type": "summary", "summary": "x"}),
        json!({"type": "last-prompt", "lastPrompt": "x"}),
        json!({"type": "system", "uuid": "s", "content": "x"}),
        json!({"type": "attachment", "uuid": "t", "timestamp": T0,
               "attachment": {"type": "hook_output", "content": "x"}}),
    ]);
    assert!(read(&format!("{text}\n\n")).expect("reads").is_empty());
}

#[test]
fn a_person_said_and_answered_words_are_what_paths_are_looked_for_in() {
    let events = read(&lines(&[
        typed("a", T0, "said"),
        round("r", T1, "not a label", "Red", Some("a note")),
    ]))
    .expect("reads");
    assert_eq!(events[0].words(), ["said"]);
    // The chosen label is the agent's word and is not his.
    assert_eq!(events[1].words(), ["not a label", "a note"]);
}
