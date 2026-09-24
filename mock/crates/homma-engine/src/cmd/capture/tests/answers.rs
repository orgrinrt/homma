//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Ask rounds: which tool results are one, and how an answer string splits
//! into the labels picked and what the person typed.

use serde_json::json;

use super::*;
use crate::cmd::capture::transcript::{
    Answer,
    Choice,
    Event,
    Happened,
    NOTES_ONLY,
    Takes,
    answered,
    read,
};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";

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
fn an_answered_round_is_read_whole_off_its_result() {
    let events = events(&lines(&[
        asking("r"),
        round("r", T0, "Right", "Red, Blue", Some("  a note ")),
    ]))
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
    assert_eq!(qs[0].notes.as_deref(), Some("  a note "));
    assert_eq!(qs[1].answer, chose(&["Red", "Blue"]));
    assert_eq!(qs[1].notes, None);
}

#[test]
fn an_answer_is_labels_then_what_was_typed() {
    let o = options(&["A", "B", "C, D"]);
    assert_eq!(answered("A", &o, Takes::Several), chose(&["A"]));
    assert_eq!(answered("A, B", &o, Takes::Several), chose(&["A", "B"]));
    assert_eq!(answered("B, A", &o, Takes::Several), chose(&["B", "A"]));
    // A label with the separator inside it is one label, bare or quoted.
    assert_eq!(answered("C, D", &o, Takes::Several), chose(&["C, D"]));
    assert_eq!(answered("\"C, D\"", &o, Takes::Several), chose(&["C, D"]));
    assert_eq!(
        answered("A, \"C, D\"", &o, Takes::Several),
        chose(&["A", "C, D"])
    );
    assert_eq!(
        answered("\"C, D\", B", &o, Takes::Several),
        chose(&["C, D", "B"])
    );
    // The harness's own words for nothing picked are nobody's answer.
    assert_eq!(answered(NOTES_ONLY, &o, Takes::Several), Answer::default());
    assert_eq!(answered("", &o, Takes::Several), Answer::default());
    // What follows the labels is the person's, and only that.
    assert_eq!(answered("A, and more", &o, Takes::Several), Answer {
        chose: vec!["A".into()],
        typed: Some("and more".into()),
    });
    assert_eq!(
        answered("A, B, but only B first", &o, Takes::Several),
        Answer {
            chose: vec!["A".into(), "B".into()],
            typed: Some("but only B first".into()),
        }
    );
    assert_eq!(answered("\"C, D\", why not", &o, Takes::Several), Answer {
        chose: vec!["C, D".into()],
        typed: Some("why not".into()),
    });
    // A label must end at the separator or the end, so a word that starts
    // with one is not it; nor does case fold.
    assert_eq!(answered("Apple", &o, Takes::Several), typed_only("Apple"));
    assert_eq!(answered("A,B", &o, Takes::Several), typed_only("A,B"));
    assert_eq!(answered("a", &o, Takes::Several), typed_only("a"));
    assert_eq!(
        answered("just merge both now", &o, Takes::Several),
        typed_only("just merge both now")
    );
    // A quote opened and never closed on a label is typed.
    assert_eq!(
        answered("\"C, D, x", &o, Takes::Several),
        typed_only("\"C, D, x")
    );
}

#[test]
fn the_longest_label_that_fits_is_taken_first() {
    // "Yes" would fit the front of both; only the longer leaves the rest a
    // label too.
    let o = options(&["Yes", "Yes, and ship it", "No"]);
    assert_eq!(
        answered("Yes, and ship it", &o, Takes::Several),
        chose(&["Yes, and ship it"])
    );
    assert_eq!(
        answered("Yes, and ship it, No", &o, Takes::Several),
        chose(&["Yes, and ship it", "No"])
    );
    assert_eq!(
        answered("Yes, No", &o, Takes::Several),
        chose(&["Yes", "No"])
    );
    assert_eq!(answered("Yes, and wait", &o, Takes::Several), Answer {
        chose: vec!["Yes".into()],
        typed: Some("and wait".into()),
    });
}

#[test]
fn a_single_answer_is_a_label_or_the_persons_words_whole() {
    let o = options(&["Yes", "No", "C, D"]);
    let one = |a: &str| answered(a, &o, Takes::One);
    assert_eq!(one("Yes"), chose(&["Yes"]));
    assert_eq!(one("C, D"), chose(&["C, D"]));
    // What the multi-select reading would split is one sentence here.
    assert_eq!(
        one("Yes, but only after the release"),
        typed_only("Yes, but only after the release")
    );
    assert_eq!(
        answered("Yes, but only after the release", &o, Takes::Several),
        Answer {
            chose: vec!["Yes".into()],
            typed: Some("but only after the release".into()),
        }
    );
    assert_eq!(one("Yes, No"), typed_only("Yes, No"));
    assert_eq!(one("\"C, D\""), typed_only("\"C, D\""));
    assert_eq!(one("yes"), typed_only("yes"));
    assert_eq!(one(NOTES_ONLY), Answer::default());
    assert_eq!(one(""), Answer::default());
}

#[test]
fn a_round_reads_which_questions_take_several() {
    // The fixture's first question takes one, its second several; the same
    // answer string reads differently under each.
    let got = events(&lines(&[
        asking("r"),
        round("r", T0, "Left, Right", "Red, Blue", None),
    ]))
    .expect("reads");
    let Happened::Answered(qs) = &got[0].what else {
        panic!("not a round");
    };
    assert_eq!(qs[0].answer, typed_only("Left, Right"));
    assert_eq!(qs[1].answer, chose(&["Red", "Blue"]));
}

#[test]
fn a_real_multi_select_with_typed_text_after_it_splits_where_the_typing_starts() {
    // The shape read off a real round: quoted labels holding the separator,
    // then a sentence of the person's.
    let o = options(&[
        "~/Dev/staging generally",
        "~/.meet, properly this time",
        "loru itself, the rest of it",
        "Nothing more",
    ]);
    let a = "~/Dev/staging generally, \"~/.meet, properly this time\", \"loru itself, the rest of it\", Just probably worth it to grep any keywords from ~/Dev in general.";
    assert_eq!(answered(a, &o, Takes::Several), Answer {
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
    let events = events(&lines(&[asking("r"), r])).expect("reads");
    let Happened::Answered(qs) = &events[0].what else {
        panic!("not a round");
    };
    assert_eq!(qs[1].answer, Answer::default());
    assert_eq!(qs[0].notes, None);
}

#[test]
fn a_round_nobody_answered_said_nothing() {
    // The harness's timeout, as real results carry it: no answers at all.
    let mut r = round("r", T0, "Left", "Red", None);
    r["toolUseResult"]["answers"] = json!({});
    r["toolUseResult"]["afkTimeoutMs"] = json!(600000);
    assert!(events(&lines(&[asking("r"), r])).expect("reads").is_empty());
    // The controls: a note alone, or one answer alone, is something said.
    let mut noted = round("r", T0, "Left", "Red", Some("only this"));
    noted["toolUseResult"]["answers"] = json!({});
    assert_eq!(
        events(&lines(&[asking("r"), noted])).expect("reads").len(),
        1
    );
    let mut one = round("r", T0, "Left", "Red", None);
    one["toolUseResult"]["answers"]
        .as_object_mut()
        .expect("a map")
        .remove("Which way?");
    assert_eq!(events(&lines(&[asking("r"), one])).expect("reads").len(), 1);
}

#[test]
fn only_a_result_answering_an_ask_call_is_read_as_a_round() {
    // Another tool's result shaped like a round, malformed or whole, is not
    // one: `answers` without `questions`, and a whole round's shape under a
    // call to some other tool.
    let whole = round("x", T1, "Left", "Red", None)["toolUseResult"].clone();
    for result in [json!({"collection": "x", "answers": ["a"]}), whole] {
        let other = json!({"type": "assistant", "uuid": "c", "timestamp": T0,
            "message": {"content": [{"type": "tool_use", "id": "t9", "name": "ArtifactData", "input": {}}]}});
        let line = json!({"type": "user", "uuid": "b", "timestamp": T1,
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t9"}]},
            "toolUseResult": result});
        let got = events(&lines(&[other, line])).expect("another tool's result is not refused");
        assert!(got.is_empty(), "{got:?}");
    }
    // A round whose call the transcript never shows is not read either.
    assert!(
        events(&lines(&[round("r", T1, "Left", "Red", None)]))
            .expect("reads")
            .is_empty()
    );
    // The control: the same result answering an ask call is a round.
    let got = events(&lines(&[asking("x"), round("x", T1, "Left", "Red", None)])).expect("reads");
    assert!(matches!(got[0].what, Happened::Answered(_)), "{got:?}");
    // And an ask rejected carries a string, not a round.
    let rejected = json!({"type": "user", "uuid": "b", "timestamp": T1,
        "message": {"content": [{"type": "tool_result", "tool_use_id": "ask-x", "is_error": true}]},
        "toolUseResult": "User rejected tool use"});
    assert!(
        events(&lines(&[asking("x"), rejected]))
            .expect("reads")
            .is_empty()
    );
}
