//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which lines of a transcript are the person's, and what rides with them.
//! Ask rounds are `answers.rs`.

use serde_json::json;

use super::*;
use crate::cmd::capture::transcript::{Event, Happened, read};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

fn events(text: &str) -> anyhow::Result<Vec<Event>> {
    read(text).map(|t| t.events)
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
    // Every one of these is a typed line in the transcript and none is the
    // person's: hook feedback, a compaction summary, a task notification,
    // command output, and a prompt the harness sends itself.
    let not_theirs = [
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
    assert!(events(&lines(&not_theirs)).expect("reads").is_empty());
}

#[test]
fn a_slash_command_the_person_typed_is_theirs() {
    // The control for the test above: the text filter it replaces would drop
    // all of these, and the origin keeps them. A slash command a tool typed
    // is taken out by the tool's own record, never by its shape.
    let events = events(&lines(&[
        typed("a", T0, "/compact \"keep the goal\""),
        typed("b", T1, "<not a tag> but theirs"),
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
        "<not a tag> but theirs",
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
fn a_queued_command_that_is_not_the_persons_is_passed_over() {
    let mut note = queued("q", T0, "<task-notification>x</task-notification>");
    note["attachment"]["origin"] = json!(null);
    note["attachment"]["commandMode"] = json!("task-notification");
    let mut peer = queued("p", T0, "from another session");
    peer["attachment"]["origin"] = json!({"kind": "peer", "from": "uds:/tmp/x"});
    assert!(events(&lines(&[note, peer])).expect("reads").is_empty());
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

/// A result line answering the call `asking("b")` makes, carrying `result`.
fn answering_b(result: serde_json::Value) -> String {
    json!({"type": "user", "uuid": "b", "timestamp": T1,
           "message": {"content": [{"type": "tool_result", "tool_use_id": "ask-b"}]},
           "toolUseResult": result})
    .to_string()
}

#[test]
fn a_malformed_line_of_a_read_shape_is_refused_by_number() {
    // Every case is line 3: a typed line, the agent's ask call, then the case.
    let cases = [
        ("not json", "line 3 is not JSON".to_string()),
        ("[1, 2]", "line 3 is not an object".to_string()),
        (
            &*json!({"type": "user", "uuid": "b", "origin": {"kind": "human"},
                     "message": {"content": "no stamp"}})
            .to_string(),
            "no `timestamp`".to_string(),
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
            &*answering_b(json!({"answers": {}})),
            "no `questions`".to_string(),
        ),
        (
            &*answering_b(json!({"answers": [], "questions": []})),
            "`answers` is not a map".to_string(),
        ),
        (
            &*answering_b(json!({"answers": {}, "questions": [{"question": "q", "options": []}]})),
            "no `header`".to_string(),
        ),
        (
            &*answering_b(json!({"answers": {}, "questions": [{"question": "q", "header": "h"}]})),
            "has no `options`".to_string(),
        ),
        (
            &*answering_b(json!({"answers": {"q": 3}, "questions": [
                {"question": "q", "header": "h", "multiSelect": false,
                 "options": [{"label": "l", "description": "d"}]}]})),
            "is not text".to_string(),
        ),
        (
            &*answering_b(json!({"answers": {"q": "l"}, "questions": [
                {"question": "q", "header": "h", "options": [{"label": "l", "description": "d"}]}]})),
            "no `multiSelect`".to_string(),
        ),
        (
            &*answering_b(json!({"answers": {}, "questions": [
                {"question": "q", "header": "h", "options": [{"label": "l"}]}]})),
            "no `description`".to_string(),
        ),
    ];
    for (line, want) in cases {
        let text = format!("{}\n{}\n{line}\n", typed("a", T0, "fine"), asking("b"));
        let err = format!("{:#}", read(&text).expect_err(line));
        assert!(err.contains(&want), "{line}: {err}");
        assert!(err.contains("line 3"), "{line}: {err}");
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
        asking("r"),
        round("r", T1, "not a label", "Red", Some("a note")),
    ]))
    .expect("reads");
    assert_eq!(got[0].words(), ["said"]);
    // The chosen label is the agent's word and is not the person's.
    assert_eq!(got[1].words(), ["not a label", "a note"]);
    // Nor when the person typed after it: the typed part alone is theirs.
    let mixed = events(&lines(&[
        asking("r"),
        round("r", T0, "Left", "Red, and GOAL.md", None),
    ]))
    .expect("reads");
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
fn a_queued_message_written_as_two_attachments_is_read_once() {
    // The second shape the harness writes: two attachments, different uuids,
    // one `source_uuid`, and no typed line anywhere.
    let mut first = queued("q1", T0, "did something change there");
    first["attachment"]["source_uuid"] = json!("s");
    let mut second = queued("q2", T1, "did something change there");
    second["attachment"]["source_uuid"] = json!("s");
    let got = events(&lines(&[
        first.clone(),
        said_by_agent("x", T1, "working"),
        second.clone(),
    ]))
    .expect("reads");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].line, 0);
    // And all three copies together, in any order, are still one.
    let twin = typed("s", T2, "did something change there");
    for order in [
        [first.clone(), second.clone(), twin.clone()],
        [twin.clone(), first.clone(), second.clone()],
        [second.clone(), twin.clone(), first.clone()],
    ] {
        assert_eq!(events(&lines(&order)).expect("reads").len(), 1);
    }
}

#[test]
fn a_queued_message_with_an_image_keeps_its_text() {
    // The shapes counted in real transcripts: text then image, image then
    // text, several images and a text, and an image alone.
    let image = json!({"type": "image", "source": {"type": "base64", "data": "x"}});
    let text = |t: &str| json!({"type": "text", "text": t});
    let with = |uuid: &str, blocks: serde_json::Value| {
        let mut q = queued(uuid, T0, "");
        q["attachment"]["prompt"] = blocks;
        q
    };
    let got = events(&lines(&[
        with("a", json!([text("look at this"), image.clone()])),
        with("b", json!([image.clone(), text("and this")])),
        with(
            "c",
            json!([image.clone(), image.clone(), text("one"), text("two")]),
        ),
        with("d", json!([image.clone()])),
        with("e", json!([])),
    ]))
    .expect("reads");
    let words: Vec<&str> = got.iter().map(said).collect();
    assert_eq!(words, ["look at this", "and this", "one\n\ntwo"]);
    // A prompt that is neither is refused by line, like any other bad field.
    let mut bad = queued("f", T0, "");
    bad["attachment"]["prompt"] = json!(7);
    let err = format!("{:#}", read(&lines(&[bad])).expect_err("refused"));
    assert!(
        err.contains("line 1") && err.contains("neither text nor blocks"),
        "{err}"
    );
}

#[test]
fn a_twin_is_only_what_the_attachment_names() {
    // The control: an attachment naming some other uuid, or none, leaves a
    // typed line with the same words alone, since saying a thing twice is the
    // person's to do.
    let mut named_other = queued("q", T0, "again");
    named_other["attachment"]["source_uuid"] = json!("elsewhere");
    let got = events(&lines(&[named_other, typed("t", T1, "again")])).expect("reads");
    assert_eq!(got.len(), 2);
    let got = events(&lines(&[queued("q", T0, "again"), typed("t", T1, "again")])).expect("reads");
    assert_eq!(got.len(), 2);
    // A typed line whose uuid a non-human attachment names is still the
    // person's.
    let mut note = queued("n", T0, "<task-notification>x</task-notification>");
    note["attachment"]["origin"] = json!(null);
    note["attachment"]["source_uuid"] = json!("t");
    let got = events(&lines(&[note, typed("t", T1, "mine")])).expect("reads");
    assert_eq!(got.len(), 1);
}
