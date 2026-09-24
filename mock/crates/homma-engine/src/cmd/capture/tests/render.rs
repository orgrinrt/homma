//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use jiff::tz::{TimeZone, offset};

use super::*;
use crate::cmd::capture::render::{Capture, fenced, file_name, kind, quote, render};
use crate::cmd::capture::transcript::read;

/// The reverse of a blockquote: what the voice tool reads back out of one.
fn unquote(block: &str) -> String {
    block
        .strip_suffix('\n')
        .unwrap_or(block)
        .split('\n')
        .map(|l| {
            l.strip_prefix("> ")
                .unwrap_or_else(|| l.strip_prefix('>').expect("a quote line"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every blockquote in `text` as the voice tool finds them
/// (`.shared/scripts/voice/src/corpus/remarks.ts`, `quotes`): a run of lines
/// opening with `>`, wherever they sit, and a blank line joins two runs when
/// the line after it opens with `>` again.
fn quotes(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut run = String::new();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with('>') {
            run.push_str(l);
            run.push('\n');
        } else if !run.is_empty() {
            if l.trim().is_empty() && lines.get(i + 1).is_some_and(|n| n.starts_with('>')) {
                run.push_str(">\n");
                continue;
            }
            if l.trim().is_empty() {
                continue;
            }
            out.push(unquote(&run));
            run.clear();
        }
    }
    if !run.is_empty() {
        out.push(unquote(&run));
    }
    out
}

#[test]
fn a_blockquote_strips_back_to_the_exact_string() {
    for text in [
        "one line",
        "two\nlines",
        "a blank\n\nbetween",
        "ends in a newline\n",
        "\nstarts with one",
        "  leading and trailing  ",
        "> already quoted",
        "",
        "\r\nwindows\r\n",
    ] {
        assert_eq!(unquote(&quote(text)), text, "{text:?}");
    }
}

#[test]
fn a_fence_outgrows_any_backticks_inside_it() {
    assert!(fenced("plain").starts_with("```text\n"));
    let four = fenced("has ``` inside");
    assert!(
        four.starts_with("````text\n") && four.ends_with("\n````\n"),
        "{four}"
    );
    let six = fenced("has ````` five");
    assert!(six.starts_with("``````text\n"), "{six}");
    // The text is inside whole, and a trailing newline is not doubled.
    assert_eq!(fenced("x\n"), "```text\nx\n```\n");
    assert_eq!(fenced("x"), "```text\nx\n```\n");
}

#[test]
fn a_file_is_named_for_the_local_minute() {
    let at = "2026-09-24T14:37:59Z".parse().unwrap();
    let helsinki = TimeZone::fixed(offset(3));
    assert_eq!(file_name(at, &helsinki, "a-b"), "202609241737_a-b.md");
    assert_eq!(file_name(at, &TimeZone::UTC, "a-b"), "202609241437_a-b.md");
}

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:05:00.000Z";

fn made(events: &[crate::cmd::capture::transcript::Event], bears_on: &[String]) -> String {
    render(&Capture {
        title: "A title",
        session: "sess",
        events,
        bears_on,
        zone: &TimeZone::fixed(offset(3)),
    })
}

#[test]
fn only_the_persons_words_are_blockquotes() {
    let text = lines(&[
        said_by_agent("x", T0, "my question, with\n> a quote of my own"),
        typed("a", T0, "his words\n\nsecond paragraph"),
        said_by_agent("y", T1, "put to him"),
        round("r", T1, "freely typed", "Red", Some("his note")),
        typed("b", T1, "more of his"),
    ]);
    let out = made(&read(&text).unwrap(), &[]);
    assert_eq!(quotes(&out), [
        "his words\n\nsecond paragraph",
        "freely typed",
        "his note",
        "more of his",
    ]);
    // The agent's words are there, fenced and off the margin, its own `>`
    // line included.
    assert!(
        out.contains("  ```text\n  my question, with\n  > a quote of my own\n  ```"),
        "{out}"
    );
    assert!(out.contains("  ```text\n  Which way?\n  ```"), "{out}");
}

#[test]
fn an_agent_line_opening_with_a_quote_mark_would_be_his_at_the_margin() {
    // The control for the indent: the same text fenced at the margin is read
    // as a remark of his by the voice tool's own rule.
    let at_margin = fenced("mine\n> not his");
    assert_eq!(quotes(&at_margin), ["not his"]);
    let aside = crate::cmd::capture::render::aside("mine\n> not his");
    assert!(quotes(&aside).is_empty(), "{aside}");
    assert_eq!(aside, "  ```text\n  mine\n  > not his\n  ```\n");
}

#[test]
fn two_quotes_of_his_never_run_together() {
    // The voice tool joins quotes across one blank line, so what separates
    // two of them in a capture has to be a line of text.
    let text = lines(&[
        typed("a", T0, "one"),
        typed("b", T0, "two"),
        round("r", T1, "typed", "Red", Some("note")),
    ]);
    let out = made(&read(&text).unwrap(), &[]);
    assert_eq!(quotes(&out), ["one", "two", "typed", "note"]);
}

#[test]
fn the_front_matter_carries_the_session_and_the_last_instant() {
    let text = lines(&[typed("a", T0, "one"), typed("b", T1, "two")]);
    let out = made(&read(&text).unwrap(), &[
        "muisti".into(),
        ".shared/x.md".into(),
    ]);
    assert!(
        out.starts_with(
            "---\nwhen: 2026-09-24 17:00\nkind: unprompted\nsource: sess\n\
         through: 2026-09-24T14:05:00Z\nbears_on:\n  - muisti\n  - .shared/x.md\n\
         tags: [chat]\n---\n\n# A title\n\n"
        ),
        "{out}"
    );
    assert!(out.ends_with("> two\n"), "{out:?}");
    let none = made(&read(&text).unwrap(), &[]);
    assert!(none.contains("\nbears_on: []\n"), "{none}");
}

#[test]
fn the_kind_says_what_the_capture_holds() {
    let said = read(&lines(&[typed("a", T0, "x")])).unwrap();
    let asked = read(&lines(&[round("r", T0, "Left", "Red", None)])).unwrap();
    let both = read(&lines(&[
        typed("a", T0, "x"),
        round("r", T1, "Left", "Red", None),
    ]))
    .unwrap();
    assert_eq!(kind(&said), "unprompted");
    assert_eq!(kind(&asked), "ask");
    assert_eq!(kind(&both), "mixed");
}

#[test]
fn every_answer_kind_is_written_as_what_it_is() {
    let chose = made(
        &read(&lines(&[round("r", T0, "Right", "Red, Blue", None)])).unwrap(),
        &[],
    );
    assert!(chose.contains("Chosen:\n\n- Right\n"), "{chose}");
    assert!(chose.contains("Chosen:\n\n- Red\n- Blue\n"), "{chose}");
    assert!(quotes(&chose).is_empty(), "{chose}");

    let notes_only = made(
        &read(&lines(&[round(
            "r",
            T0,
            "(notes only)",
            "Red",
            Some("only this"),
        )]))
        .unwrap(),
        &[],
    );
    assert!(
        notes_only.contains("No option chosen.\n\nNotes:\n\n> only this\n"),
        "{notes_only}"
    );
    assert!(!notes_only.contains("(notes only)"), "{notes_only}");

    let typed_answer = made(
        &read(&lines(&[round("r", T0, "my own", "Red", None)])).unwrap(),
        &[],
    );
    assert!(
        typed_answer.contains("Answered in their own words:\n\n> my own\n"),
        "{typed_answer}"
    );
}

#[test]
fn every_option_is_written_with_its_description_and_preview() {
    let out = made(
        &read(&lines(&[round("r", T0, "Left", "Red", None)])).unwrap(),
        &[],
    );
    assert!(out.contains("### Way\n"), "{out}");
    assert!(out.contains("1. Left\n\n   Go left.\n"), "{out}");
    // The preview holds a fence of its own, so its fence is longer.
    assert!(
        out.contains("2. Right\n\n   Go right.\n\n   ````text\n   ```\n   R\n   ```\n   ````\n"),
        "{out}"
    );
    assert!(out.contains("### Colours\n"), "{out}");
}

#[test]
fn the_same_context_is_not_written_twice_in_a_row() {
    let text = lines(&[
        said_by_agent("x", T0, "once"),
        typed("a", T0, "first"),
        typed("b", T1, "second"),
        said_by_agent("y", T1, "then this"),
        typed("c", T1, "third"),
    ]);
    let out = made(&read(&text).unwrap(), &[]);
    assert_eq!(out.matches("## What was going on").count(), 2, "{out}");
    assert_eq!(out.matches("  ```text\n  once\n  ```").count(), 1, "{out}");
    assert!(out.contains("## Said unprompted at 17:00\n"), "{out}");
    assert!(out.contains("## Said unprompted at 17:05\n"), "{out}");
}
