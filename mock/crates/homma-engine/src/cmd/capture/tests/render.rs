//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use jiff::tz::{TimeZone, offset};
use serde_json::json;

use super::*;
use crate::cmd::capture::render::{Capture, fenced, file_name, kind, quote, render};
use crate::cmd::capture::transcript::{self, Event};

fn read(text: &str) -> anyhow::Result<Vec<Event>> {
    transcript::read(text).map(|t| t.events)
}

fn events(text: &str) -> Vec<Event> {
    read(text).expect("reads")
}

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

/// Every blockquote in `text` as the voice tool finds them, rule for rule
/// with `quotes` in the workspace's `.shared/scripts/voice/src/corpus/remarks.ts`:
/// a line matching `/^>\s?(.*)$/s` is a quote line holding the capture, a run
/// of them is one quote, a blank line inside a run is skipped and adds a break
/// when the line after it is a quote line again, any other line ends the run,
/// and the text has its leading and trailing newlines trimmed and is dropped
/// when it is only whitespace.
fn quotes(text: &str) -> Vec<String> {
    fn quoted(l: &str) -> Option<&str> {
        let rest = l.strip_prefix('>')?;
        Some(rest.strip_prefix(char::is_whitespace).unwrap_or(rest))
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut held: Option<Vec<&str>> = None;
    let close = |held: &mut Option<Vec<&str>>, out: &mut Vec<String>| {
        if let Some(h) = held.take() {
            let t = h.join("\n");
            let t = t.trim_end_matches('\n').trim_start_matches('\n');
            if !t.trim().is_empty() {
                out.push(t.to_string());
            }
        }
    };
    for (i, l) in lines.iter().enumerate() {
        if let Some(q) = quoted(l) {
            held.get_or_insert_with(Vec::new).push(q);
            continue;
        }
        if let Some(h) = held.as_mut() {
            if l.trim().is_empty() {
                if lines.get(i + 1).is_some_and(|n| n.starts_with('>')) {
                    h.push("");
                }
                continue;
            }
            close(&mut held, &mut out);
        }
    }
    close(&mut held, &mut out);
    out
}

#[test]
fn the_voice_tools_reader_takes_a_blockquote_back_to_the_exact_string() {
    // The same round trip as below, through the reader that consumes it, for
    // every string whose ends are not blank lines, which it trims.
    for text in [
        "one line",
        "two\nlines",
        "a blank\n\nbetween",
        "  leading and trailing  ",
        "> already quoted",
        "\r\nwindows\r",
        "\ttab first",
    ] {
        assert_eq!(quotes(&quote(text)), [text], "{text:?}");
    }
}

/// Every string of up to `most` characters over `alphabet`.
fn every_string(alphabet: &[char], most: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut last = vec![String::new()];
    for _ in 0 .. most {
        last = last
            .iter()
            .flat_map(|s| {
                alphabet.iter().map(move |c| {
                    let mut t = s.clone();
                    t.push(*c);
                    t
                })
            })
            .collect();
        all.extend(last.iter().cloned());
    }
    all
}

#[test]
fn every_short_string_survives_the_quote_both_ways() {
    // Exhaustive over the characters a line break or a quote mark is made of,
    // plus a letter, a space and a tab: both readers, every string.
    let alphabet = ['a', ' ', '\n', '\r', '>', '\t', '\u{2028}', '\u{85}'];
    let all = every_string(&alphabet, 6);
    assert_eq!(all.len(), (0 ..= 6).map(|n| 8usize.pow(n)).sum::<usize>());
    for text in &all {
        let q = quote(text);
        assert_eq!(unquote(&q), *text, "{text:?}");
        // The voice tool trims newlines off the ends and drops a quote that is
        // only whitespace; everything else comes back whole.
        let kept = text.trim_start_matches('\n').trim_end_matches('\n');
        let want: Vec<&str> = if kept.trim().is_empty() { vec![] } else { vec![kept] };
        assert_eq!(quotes(&q), want, "{text:?}");
    }
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
        through: "last-uuid",
        events,
        bears_on,
        zone: &TimeZone::fixed(offset(3)),
    })
}

#[test]
fn only_the_persons_words_are_blockquotes() {
    let text = lines(&[
        said_by_agent("x", T0, "my question, with\n> a quote of my own"),
        typed("a", T0, "their words\n\nsecond paragraph"),
        said_by_agent("y", T1, "put to them"),
        asking("r"),
        round("r", T1, "freely typed", "Red", Some("their note")),
        typed("b", T1, "more of theirs"),
    ]);
    let out = made(&read(&text).unwrap(), &[]);
    assert_eq!(quotes(&out), [
        "their words\n\nsecond paragraph",
        "freely typed",
        "their note",
        "more of theirs",
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
fn agent_text_in_every_place_it_sits_stays_off_the_margin() {
    // The agent's turn, the question, its header, an option's label (listed
    // and chosen), its description, its preview, and the title: each carries
    // a line break and a quote mark, and the person's typed answer is the
    // only quote the file holds.
    const LEAK: &str = "\n> leak";
    let call = json!({"type": "assistant", "uuid": "c", "timestamp": T0,
    "message": {"content": [
        {"type": "text", "text": format!("turn{LEAK}")},
        {"type": "tool_use", "id": "ask-c", "name": "AskUserQuestion", "input": {}},
    ]}});
    let label = format!("label{LEAK}");
    let result = json!({"type": "user", "uuid": "r", "timestamp": T1,
    "message": {"content": [{"type": "tool_result", "tool_use_id": "ask-c"}]},
    "toolUseResult": {
        "questions": [
            {"question": format!("question{LEAK}"), "header": format!("header{LEAK}"),
             "multiSelect": true, "options": [
                {"label": label, "description": format!("description{LEAK}"),
                 "preview": format!("preview{LEAK}")},
            ]},
        ],
        "answers": {format!("question{LEAK}"): format!("{label}, typed")},
        "annotations": {},
    }});
    let events = events(&lines(&[call, result]));
    let out = render(&Capture {
        title:    &format!("title{LEAK}"),
        session:  "sess",
        through:  "r",
        events:   &events,
        bears_on: &[],
        zone:     &TimeZone::UTC,
    });
    assert_eq!(quotes(&out), ["typed"], "{out}");
    // The one-line fields are there, their breaks spelled out.
    for field in [
        "# title\\n> leak\n",
        "### header\\n> leak\n",
        "1. label\\n> leak\n",
        "- label\\n> leak\n",
    ] {
        assert!(out.contains(field), "{field:?} in {out}");
    }
}

#[test]
fn an_agent_line_opening_with_a_quote_mark_would_be_the_persons_at_the_margin() {
    // The control for the indent: the same text fenced at the margin is read
    // as one of the person's remarks by the voice tool's own rule.
    let at_margin = fenced("mine\n> not theirs");
    assert_eq!(quotes(&at_margin), ["not theirs"]);
    let aside = crate::cmd::capture::render::aside("mine\n> not theirs");
    assert!(quotes(&aside).is_empty(), "{aside}");
    assert_eq!(aside, "  ```text\n  mine\n  > not theirs\n  ```\n");
    // And the control for the one-line fields: unescaped, a label is a quote.
    assert_eq!(quotes("1. label\n> leak\n"), ["leak"]);
}

#[test]
fn two_of_the_persons_quotes_never_run_together() {
    // The voice tool joins quotes across one blank line, so what separates
    // two of them in a capture has to be a line of text.
    let text = lines(&[
        typed("a", T0, "one"),
        typed("b", T0, "two"),
        asking("r"),
        round("r", T1, "typed", "Red", Some("note")),
    ]);
    let out = made(&read(&text).unwrap(), &[]);
    assert_eq!(quotes(&out), ["one", "two", "typed", "note"]);
}

#[test]
fn the_front_matter_carries_the_session_and_the_last_line_read() {
    let text = lines(&[typed("a", T0, "one"), typed("b", T1, "two")]);
    let out = made(&events(&text), &["muisti".into(), "example/x.md".into()]);
    assert!(
        out.starts_with(
            "---\nwhen: 2026-09-24 17:00\nkind: unprompted\nsource: sess\n\
         through: last-uuid\nbears_on:\n  - muisti\n  - example/x.md\n\
         tags: [chat]\n---\n\n# A title\n\n"
        ),
        "{out}"
    );
    assert!(out.ends_with("> two\n"), "{out:?}");
    let none = made(&events(&text), &[]);
    assert!(none.contains("\nbears_on: []\n"), "{none}");
}

#[test]
fn the_kind_says_what_the_capture_holds() {
    let said = read(&lines(&[typed("a", T0, "x")])).unwrap();
    let asked = read(&lines(&[asking("r"), round("r", T0, "Left", "Red", None)])).unwrap();
    let both = read(&lines(&[
        typed("a", T0, "x"),
        asking("r"),
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
        &read(&lines(&[
            asking("r"),
            round("r", T0, "Right", "Red, Blue", None),
        ]))
        .unwrap(),
        &[],
    );
    assert!(chose.contains("Chosen:\n\n- Right\n"), "{chose}");
    assert!(chose.contains("Chosen:\n\n- Red\n- Blue\n"), "{chose}");
    assert!(quotes(&chose).is_empty(), "{chose}");

    let notes_only = made(
        &read(&lines(&[
            asking("r"),
            round("r", T0, "(notes only)", "Red", Some("only this")),
        ]))
        .unwrap(),
        &[],
    );
    assert!(
        notes_only.contains("No option chosen.\n\nNotes:\n\n> only this\n"),
        "{notes_only}"
    );
    assert!(!notes_only.contains("(notes only)"), "{notes_only}");

    let typed_answer = made(
        &read(&lines(&[
            asking("r"),
            round("r", T0, "my own", "Red", None),
        ]))
        .unwrap(),
        &[],
    );
    assert!(
        typed_answer.contains("Answered in their own words:\n\n> my own\n"),
        "{typed_answer}"
    );
    assert!(
        !typed_answer.contains("Chosen:\n\n- my own"),
        "{typed_answer}"
    );

    // Labels and then the person's words: the labels named, only their words
    // quoted.
    let mixed = made(
        &events(&lines(&[
            asking("r"),
            round("r", T0, "Left", "Red, \"Blue\", and green", Some("n")),
        ])),
        &[],
    );
    assert!(
        mixed.contains(
            "### Colours\n\n  ```text\n  Which colours?\n  ```\n\nOptions offered:\n\n1. Red\n\n   Warm.\n\n2. Blue\n\n   Cold.\n\n\
             Chosen:\n\n- Red\n- Blue\n\nAnswered in their own words:\n\n> and green\n"
        ),
        "{mixed}"
    );
    // The first question's notes, then the second's typed part.
    assert_eq!(quotes(&mixed), ["n", "and green"]);
}

#[test]
fn every_option_is_written_with_its_description_and_preview() {
    let out = made(
        &read(&lines(&[asking("r"), round("r", T0, "Left", "Red", None)])).unwrap(),
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
