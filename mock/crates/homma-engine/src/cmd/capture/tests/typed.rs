//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Lines a tool typed into the terminal, taken back out of what the person
//! said.

use std::path::Path;

use serde_json::json;

use super::*;
use crate::cmd::capture::transcript::{Event, Happened, read};
use crate::cmd::capture::typed::{Typed, beside, records, without};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

fn events(v: &[serde_json::Value]) -> Vec<Event> {
    read(&lines(v)).expect("reads").events
}

fn by_tool(at: &str, text: &str) -> Typed {
    Typed {
        at:   at.parse().expect("an instant"),
        text: text.to_string(),
    }
}

fn said(e: &[Event]) -> Vec<&str> {
    e.iter()
        .map(|e| {
            match &e.what {
                Happened::Said(w) => w.as_str(),
                Happened::Answered(_) => "<round>",
            }
        })
        .collect()
}

/// The harness's wrapper round a paste it took off.
fn pasted(id: &str, text: &str) -> String {
    format!("<pasted_content id=\"{id}\">\n{text}\n</pasted_content id=\"{id}\">")
}

#[test]
fn the_record_sits_beside_the_transcript_and_is_never_one() {
    let at = beside(Path::new("/p/abc-123.jsonl"));
    assert_eq!(at, Path::new("/p/abc-123.typed-by-tools.ndjson"));
    assert_ne!(at.extension().and_then(|e| e.to_str()), Some("jsonl"));
}

#[test]
fn records_are_read_one_a_line_in_order() {
    let text = format!(
        "{}\n\n{}\n",
        json!({"at": T0, "text": "/compact \"keep\""}),
        json!({"at": T1, "text": "Continue", "by": "self-compact"}),
    );
    assert_eq!(records(&text).expect("reads"), [
        by_tool(T0, "/compact \"keep\""),
        by_tool(T1, "Continue"),
    ]);
    assert!(records("").expect("reads").is_empty());
}

#[test]
fn a_bad_record_is_refused_by_its_line() {
    let good = json!({"at": T0, "text": "x"}).to_string();
    for (bad, want) in [
        ("not json".to_string(), "line 2 is not JSON"),
        (json!({"text": "x"}).to_string(), "line 2 has no `at`"),
        (
            json!({"at": 7, "text": "x"}).to_string(),
            "line 2 has no `at`",
        ),
        (
            json!({"at": "yesterday", "text": "x"}).to_string(),
            "line 2: `at` \"yesterday\" is not an instant",
        ),
        (json!({"at": T0}).to_string(), "line 2 has no `text`"),
        (
            json!({"at": T0, "text": 3}).to_string(),
            "line 2 has no `text`",
        ),
    ] {
        let err = format!(
            "{:#}",
            records(&format!("{good}\n{bad}\n")).expect_err(&bad)
        );
        assert!(err.contains(want), "{bad}: {err}");
    }
}

#[test]
fn a_line_a_tool_typed_is_dropped_bare_or_pasted() {
    let got = events(&[
        typed("a", T1, "/compact \"keep the goal\""),
        typed("b", T1, &pasted("3", "Continue")),
        typed("c", T2, "mine"),
    ]);
    let kept = without(got, &[
        by_tool(T0, "/compact \"keep the goal\""),
        by_tool(T0, "Continue"),
    ]);
    assert_eq!(said(&kept), ["mine"]);
}

#[test]
fn a_paste_with_line_breaks_round_its_wrapper_is_the_tools() {
    // The shape the injector's `/compact` landed in on 2026-09-25: two line
    // breaks before the wrapper and one after, which the harness put there.
    let got = events(&[
        typed("a", T1, &format!("\n\n{}\n", pasted("3", "/compact \"x\""))),
        typed("b", T1, &format!("{}\n", pasted("4", "Continue"))),
        typed("c", T2, "mine"),
    ]);
    let kept = without(got, &[
        by_tool(T0, "/compact \"x\""),
        by_tool(T0, "Continue"),
    ]);
    assert_eq!(said(&kept), ["mine"]);
}

#[test]
fn only_line_breaks_outside_the_wrapper_are_the_harnesss() {
    // The control for the arm above: anything else outside the wrapper, or line
    // breaks round a bare text, is the person's.
    let got = events(&[
        typed("a", T1, &format!(" {}", pasted("3", "Continue"))),
        typed("b", T1, &format!("{}\nand more", pasted("3", "Continue"))),
        typed("c", T1, &format!("\t{}\n", pasted("3", "Continue"))),
        typed("d", T1, "\nContinue\n"),
        typed("e", T1, &format!("\n{}\n", pasted("3", "Continue\n"))),
    ]);
    let n = got.len();
    assert_eq!(without(got, &[by_tool(T0, "Continue")]).len(), n);
}

#[test]
fn the_same_words_are_the_persons_where_no_record_takes_them() {
    // One record takes one line: the person typing the same words after it
    // keeps theirs, and so does a line stamped before the tool typed.
    let got = events(&[
        typed("a", T0, "Continue"),
        typed("b", T1, "Continue"),
        typed("c", T2, "Continue"),
    ]);
    let kept = without(got, &[by_tool(T1, "Continue")]);
    let lines: Vec<usize> = kept.iter().map(|e| e.line).collect();
    assert_eq!(lines, [0, 2]);
    // With no record, nothing is dropped.
    let got = events(&[typed("a", T0, "Continue")]);
    assert_eq!(said(&without(got, &[])), ["Continue"]);
}

#[test]
fn a_record_whose_line_never_landed_does_not_take_the_persons_later() {
    // The paste failed, so nothing matched at 14:00; the person's own
    // `Continue` at 14:02 is theirs.
    let got = events(&[typed("a", T2, "Continue")]);
    assert_eq!(said(&without(got, &[by_tool(T0, "Continue")])), [
        "Continue"
    ]);
    // The edges: at the window's end the line is the tool's, a second past it
    // the person's.
    let at_end = "2026-09-24T14:01:00.000Z";
    let past = "2026-09-24T14:01:01.000Z";
    let got = events(&[typed("a", at_end, "Continue"), typed("b", past, "Continue")]);
    let kept = without(got, &[by_tool(T0, "Continue"), by_tool(T0, "Continue")]);
    let lines: Vec<usize> = kept.iter().map(|e| e.line).collect();
    assert_eq!(lines, [1]);
}

#[test]
fn two_records_take_two_lines() {
    // One record one line: with the same text twice, both lines are the
    // tool's, and a third of the person's is kept.
    let got = events(&[
        typed("a", T0, "Continue"),
        typed("b", T0, "Continue"),
        typed("c", T0, "Continue"),
    ]);
    let kept = without(got, &[by_tool(T0, "Continue"), by_tool(T0, "Continue")]);
    let lines: Vec<usize> = kept.iter().map(|e| e.line).collect();
    assert_eq!(lines, [2]);
}

#[test]
fn a_record_after_the_line_does_not_take_it() {
    let got = events(&[typed("a", T0, "Continue")]);
    assert_eq!(said(&without(got, &[by_tool(T1, "Continue")])), [
        "Continue"
    ]);
}

#[test]
fn only_the_whole_text_or_one_whole_wrapper_matches() {
    let got = events(&[
        typed("a", T1, "Continue please"),
        typed("b", T1, " Continue"),
        typed("c", T1, &pasted("3", "Continue and more")),
        typed(
            "d",
            T1,
            &pasted("3", "Continue").replace("id=\"3\">\nC", "id=\"4\">\nC"),
        ),
        typed("e", T1, &pasted("3", &pasted("4", "Continue"))),
    ]);
    let n = got.len();
    assert_eq!(without(got, &[by_tool(T0, "Continue")]).len(), n);
}

#[test]
fn a_round_is_never_taken_for_a_typed_line() {
    let got = events(&[asking("r"), round("r", T1, "Continue", "Red", None)]);
    let kept = without(got, &[by_tool(T0, "Continue")]);
    assert_eq!(said(&kept), ["<round>"]);
}

#[test]
fn a_queued_line_a_tool_typed_is_dropped_too() {
    // The injector's lines landed as queued attachments on the day this was
    // written, while a turn ran.
    let got = events(&[queued("q1", T1, "/compact \"x\""), queued("q2", T1, "Continue")]);
    let kept = without(got, &[
        by_tool(T0, "/compact \"x\""),
        by_tool(T0, "Continue"),
    ]);
    assert!(kept.is_empty(), "{:?}", said(&kept));
}
