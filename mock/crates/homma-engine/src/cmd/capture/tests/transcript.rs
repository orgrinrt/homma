//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use serde_json::json;

use super::*;
use crate::cmd::capture::transcript::{
    Answer,
    Choice,
    Event,
    Happened,
    NOTES_ONLY,
    answered,
    read,
};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

fn events(text: &str) -> anyhow::Result<Vec<Event>> {
    read(text).map(|t| t.events)
}

fn chose(labels: &[&str]) -> Answer {
    Answer {
        chose: labels.iter().map(|l| l.to_string()).collect(),
        typed: None,
    }
}

fn typed_only(t: &str) -> Answer {
    Answer {
        chose: Vec::new(),
        typed: Some(t.to_string()),
    }
}

fn said(e: &Event) -> &str {
    match &e.what {
        Happened::Said(w) => w,
        other => panic!("not said: {other:?}"),
    }
}

#[test]
fn a_typed_line_is_read_word_for_word() {
    let words = "  Keep it,\n\nand the \"quotes\" and `ticks` and trailing space \n";
    let events = events(&lines(&[typed("a", T0, words)])).expect("reads");
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
    assert!(events(&lines(&not_his)).expect("reads").is_empty());
}

#[test]
fn a_slash_command_and_a_pasted_path_are_his_when_he_typed_them() {
    // The control for the test above: the text filter it replaces would drop
    // both of these, and the origin keeps them.
    let events = events(&lines(&[
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
    let events =
        events(&lines(&[queue, removed, queued("q", T1, "while you work")])).expect("reads");
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
    assert!(events(&lines(&[note, peer])).expect("reads").is_empty());
}

#[test]
fn a_replayed_line_is_read_once() {
    let events = events(&lines(&[typed("a", T0, "once"), typed("a", T0, "once")])).expect("reads");
    assert_eq!(events.len(), 1);
}

#[test]
fn a_sub_agents_lines_are_not_this_conversation() {
    let mut side = typed("a", T0, "from inside an agent");
    side["isSidechain"] = json!(true);
    assert!(events(&lines(&[side])).expect("reads").is_empty());
}

#[test]
fn the_agents_last_text_before_an_event_rides_with_it() {
    let events = events(&lines(&[
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
    let events = events(&lines(&[
        said_by_agent("x", T0, "kept"),
        tools_only,
        typed("a", T2, "hi"),
    ]))
    .expect("reads");
    assert_eq!(events[0].before.as_deref(), Some("kept"));
}

#[test]
fn a_first_event_has_nothing_before_it() {
    let events = events(&lines(&[typed("a", T0, "hi")])).expect("reads");
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
    let events = events(&lines(&[line])).expect("reads");
    assert_eq!(said(&events[0]), "look at this\n\nand this");
}

#[test]
fn an_image_alone_is_no_words() {
    let line = json!({"type": "user", "uuid": "a", "timestamp": T0, "origin": {"kind": "human"},
        "message": {"content": [{"type": "image", "source": {}}]}});
    assert!(events(&lines(&[line])).expect("reads").is_empty());
}

#[test]
fn an_answered_round_is_read_whole_off_its_result() {
    let events = events(&lines(&[round(
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
    assert_eq!(qs[0].answer, chose(&["Right"]));
    assert_eq!(qs[0].notes.as_deref(), Some("  his note "));
    assert_eq!(qs[1].answer, chose(&["Red", "Blue"]));
    assert_eq!(qs[1].notes, None);
}

fn options(labels: &[&str]) -> Vec<Choice> {
    labels
        .iter()
        .map(|l| {
            Choice {
                label:       l.to_string(),
                description: String::new(),
                preview:     None,
            }
        })
        .collect()
}

#[test]
fn an_answer_is_labels_then_what_was_typed() {
    let o = options(&["A", "B", "C, D"]);
    assert_eq!(answered("A", &o), chose(&["A"]));
    assert_eq!(answered("A, B", &o), chose(&["A", "B"]));
    assert_eq!(answered("B, A", &o), chose(&["B", "A"]));
    // A label with the separator inside it is one label, bare or quoted.
    assert_eq!(answered("C, D", &o), chose(&["C, D"]));
    assert_eq!(answered("\"C, D\"", &o), chose(&["C, D"]));
    assert_eq!(answered("A, \"C, D\"", &o), chose(&["A", "C, D"]));
    assert_eq!(answered("\"C, D\", B", &o), chose(&["C, D", "B"]));
    // The harness's own words for nothing picked are nobody's answer.
    assert_eq!(answered(NOTES_ONLY, &o), Answer::default());
    assert_eq!(answered("", &o), Answer::default());
    // What follows the labels is his, and only that.
    assert_eq!(answered("A, and more", &o), Answer {
        chose: vec!["A".into()],
        typed: Some("and more".into()),
    });
    assert_eq!(answered("A, B, but only B first", &o), Answer {
        chose: vec!["A".into(), "B".into()],
        typed: Some("but only B first".into()),
    });
    assert_eq!(answered("\"C, D\", why not", &o), Answer {
        chose: vec!["C, D".into()],
        typed: Some("why not".into()),
    });
    // A label must end at the separator or the end, so a word that starts
    // with one is not it; nor does case fold.
    assert_eq!(answered("Apple", &o), typed_only("Apple"));
    assert_eq!(answered("A,B", &o), typed_only("A,B"));
    assert_eq!(answered("a", &o), typed_only("a"));
    assert_eq!(
        answered("just merge both now", &o),
        typed_only("just merge both now")
    );
    // A quote opened and never closed on a label is typed.
    assert_eq!(answered("\"C, D, x", &o), typed_only("\"C, D, x"));
}

#[test]
fn the_longest_label_that_fits_is_taken_first() {
    // "Yes" would fit the front of both; only the longer leaves the rest a
    // label too.
    let o = options(&["Yes", "Yes, and ship it", "No"]);
    assert_eq!(
        answered("Yes, and ship it", &o),
        chose(&["Yes, and ship it"])
    );
    assert_eq!(
        answered("Yes, and ship it, No", &o),
        chose(&["Yes, and ship it", "No"])
    );
    assert_eq!(answered("Yes, No", &o), chose(&["Yes", "No"]));
    assert_eq!(answered("Yes, and wait", &o), Answer {
        chose: vec!["Yes".into()],
        typed: Some("and wait".into()),
    });
}

#[test]
fn a_real_multi_select_with_typed_text_after_it_splits_where_he_started_typing() {
    // The shape read off a real round: quoted labels holding the separator,
    // then a sentence of his.
    let o = options(&[
        "~/Dev/staging generally",
        "~/.meet, properly this time",
        "loru itself, the rest of it",
        "Nothing more",
    ]);
    let a = "~/Dev/staging generally, \"~/.meet, properly this time\", \"loru itself, the rest of it\", Just probably worth it to grep any keywords from ~/Dev in general.";
    assert_eq!(answered(a, &o), Answer {
        chose: vec![
            "~/Dev/staging generally".into(),
            "~/.meet, properly this time".into(),
            "loru itself, the rest of it".into(),
        ],
        typed: Some("Just probably worth it to grep any keywords from ~/Dev in general.".into()),
    });
}

#[test]
fn an_unanswered_question_is_nothing_and_empty_notes_are_none() {
    let mut r = round("r", T0, "Left", "Red", Some(""));
    r["toolUseResult"]["answers"]
        .as_object_mut()
        .expect("a map")
        .remove("Which colours?");
    let events = events(&lines(&[r])).expect("reads");
    let Happened::Answered(qs) = &events[0].what else {
        panic!("not a round");
    };
    assert_eq!(qs[1].answer, Answer::default());
    assert_eq!(qs[0].notes, None);
}

#[test]
fn a_tool_result_without_answers_is_not_a_round() {
    let line = json!({"type": "user", "uuid": "a", "timestamp": T0,
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]},
        "toolUseResult": {"stdout": "ok"}});
    assert!(events(&lines(&[line])).expect("reads").is_empty());
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
    assert!(events(&format!("{text}\n\n")).expect("reads").is_empty());
}

#[test]
fn a_person_said_and_answered_words_are_what_paths_are_looked_for_in() {
    let got = events(&lines(&[
        typed("a", T0, "said"),
        round("r", T1, "not a label", "Red", Some("a note")),
    ]))
    .expect("reads");
    assert_eq!(got[0].words(), ["said"]);
    // The chosen label is the agent's word and is not his.
    assert_eq!(got[1].words(), ["not a label", "a note"]);
    // Nor when he typed after it: the typed part alone is his.
    let mixed = events(&lines(&[round("r", T0, "Left", "Red, and GOAL.md", None)])).expect("reads");
    assert_eq!(mixed[0].words(), ["and GOAL.md"]);
}

#[test]
fn a_last_line_with_no_newline_is_still_being_written() {
    let whole = lines(&[typed("a", T0, "done"), typed("b", T1, "half")]);
    let cut = whole.trim_end_matches('\n');
    let t = read(cut).expect("reads");
    assert_eq!(t.events.len(), 1);
    assert_eq!(said(&t.events[0]), "done");
    assert_eq!(t.last.as_deref(), Some("a"));
    // Even when the unfinished part is not yet JSON.
    let broken = format!("{}{{\"type\": \"us", lines(&[typed("a", T0, "done")]));
    assert_eq!(events(&broken).expect("reads").len(), 1);
    // And a single line with no newline is nothing read yet.
    let t = read(&typed("a", T0, "alone").to_string()).expect("reads");
    assert!(t.events.is_empty() && t.last.is_none());
}

#[test]
fn every_event_carries_its_line_and_every_uuid_where_it_was_first_written() {
    let t = read(&lines(&[
        json!({"type": "summary", "summary": "no uuid"}),
        said_by_agent("x", T0, "hi"),
        typed("a", T1, "one"),
        typed("a", T1, "one"),
        queued("q", T0, "two"),
    ]))
    .expect("reads");
    let at: Vec<usize> = t.events.iter().map(|e| e.line).collect();
    assert_eq!(at, [2, 4]);
    assert_eq!(t.line_of("x"), Some(1));
    // A replay keeps the line it was first written on.
    assert_eq!(t.line_of("a"), Some(2));
    assert_eq!(t.line_of("q"), Some(4));
    assert_eq!(t.line_of("nowhere"), None);
    assert_eq!(t.last.as_deref(), Some("q"));
}

#[test]
fn the_last_uuid_is_the_last_written_whatever_its_stamp_or_kind() {
    // Stamps run backwards and the last line is a sub-agent's; the watermark
    // is still the last line in the file.
    let mut side = typed("s", T0, "from inside an agent");
    side["isSidechain"] = json!(true);
    let t = read(&lines(&[
        typed("a", T2, "late"),
        queued("q", T0, "early"),
        side,
    ]))
    .expect("reads");
    assert_eq!(t.last.as_deref(), Some("s"));
    // A replayed line last does not move it back.
    let t = read(&lines(&[
        typed("a", T0, "x"),
        typed("b", T1, "y"),
        typed("a", T0, "x"),
    ]))
    .expect("reads");
    assert_eq!(t.last.as_deref(), Some("b"));
}

#[test]
fn a_queued_message_written_twice_is_read_where_it_came_first() {
    let mut q = queued("q", T0, "while you work");
    q["attachment"]["source_uuid"] = json!("t");
    let twin = typed("t", T1, "while you work");
    // The attachment first, then the typed twin.
    let got = events(&lines(&[q.clone(), twin.clone()])).expect("reads");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].line, 0);
    // The typed line first, then the attachment naming it.
    let got = events(&lines(&[twin.clone(), q.clone()])).expect("reads");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].line, 0);
    // Apart, with other things between, still once.
    let got = events(&lines(&[
        q.clone(),
        said_by_agent("x", T1, "working"),
        typed("o", T1, "something else"),
        twin.clone(),
    ]))
    .expect("reads");
    let words: Vec<&str> = got.iter().map(said).collect();
    assert_eq!(words, ["while you work", "something else"]);
}

#[test]
fn a_twin_is_only_what_the_attachment_names() {
    // The control: an attachment naming some other uuid, or none, leaves a
    // typed line with the same words alone, since saying a thing twice is his
    // to do.
    let mut named_other = queued("q", T0, "again");
    named_other["attachment"]["source_uuid"] = json!("elsewhere");
    let got = events(&lines(&[named_other, typed("t", T1, "again")])).expect("reads");
    assert_eq!(got.len(), 2);
    let got = events(&lines(&[queued("q", T0, "again"), typed("t", T1, "again")])).expect("reads");
    assert_eq!(got.len(), 2);
    // A typed line whose uuid a non-human attachment names is still his.
    let mut note = queued("n", T0, "<task-notification>x</task-notification>");
    note["attachment"]["origin"] = json!(null);
    note["attachment"]["source_uuid"] = json!("t");
    let got = events(&lines(&[note, typed("t", T1, "mine")])).expect("reads");
    assert_eq!(got.len(), 1);
}
