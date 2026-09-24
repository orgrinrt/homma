//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The capture file itself.
//!
//! Only the person's words are blockquotes. The voice tool reads every
//! blockquote in a capture as one of their remarks, so everything the agent
//! wrote, the question and the turn before a message, goes in a fence. The
//! voice tool reads a line opening with `>` as a quote wherever it sits, fence
//! or not, so the fence is indented two spaces; markdown strips those again from
//! every line of it, and no line of the agent's opens with `>`.

use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::transcript::{Answer, Event, Happened, Question};

/// What a capture is made of before it is text.
pub struct Capture<'a> {
    pub title:    &'a str,
    pub session:  &'a str,
    pub events:   &'a [Event],
    pub bears_on: &'a [String],
    pub zone:     &'a TimeZone,
}

/// Which kinds of event a capture holds.
pub fn kind(events: &[Event]) -> &'static str {
    let said = events.iter().any(|e| matches!(e.what, Happened::Said(_)));
    let asked = events
        .iter()
        .any(|e| matches!(e.what, Happened::Answered(_)));
    match (said, asked) {
        (true, true) => "mixed",
        (false, true) => "ask",
        _ => "unprompted",
    }
}

fn local(at: Timestamp, zone: &TimeZone, format: &str) -> String {
    at.to_zoned(zone.clone()).strftime(format).to_string()
}

/// The file name: the first event's local minute, then the slug.
pub fn file_name(first: Timestamp, zone: &TimeZone, slug: &str) -> String {
    format!("{}_{slug}.md", local(first, zone, "%Y%m%d%H%M"))
}

/// `text` as a blockquote, every line of it, so stripping the `> ` back off
/// gives the string again.
pub fn quote(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        if line.is_empty() {
            out.push_str(">\n");
        } else {
            let _ = writeln!(out, "> {line}");
        }
    }
    out
}

/// `text` in a fence one backtick longer than any run of backticks inside it,
/// so nothing in the text can close it.
pub fn fenced(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat((longest + 1).max(3));
    let mut out = format!("{fence}text\n{text}");
    if !text.ends_with('\n') {
        out.push('\n');
    }
    let _ = writeln!(out, "{fence}");
    out
}

fn indented(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    text.lines()
        .map(
            |l| {
                if l.is_empty() { String::new() } else { format!("{pad}{l}") }
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// The agent's text as it sits in a capture: fenced, and indented off the
/// margin a quote is read at.
pub fn aside(text: &str) -> String {
    let mut out = indented(&fenced(text), 2);
    out.push('\n');
    out
}

fn question(out: &mut String, q: &Question) {
    let _ = writeln!(out, "### {}\n", q.header);
    out.push_str(&aside(&q.text));
    out.push_str("\nOptions offered:\n\n");
    for (i, c) in q.options.iter().enumerate() {
        let _ = writeln!(out, "{}. {}\n", i + 1, c.label);
        let _ = writeln!(out, "{}\n", indented(&c.description, 3));
        if let Some(p) = &c.preview {
            let _ = writeln!(out, "{}\n", indented(&fenced(p), 3));
        }
    }
    match &q.answer {
        Answer::Chose(labels) => {
            out.push_str("Chosen:\n\n");
            for l in labels {
                let _ = writeln!(out, "- {l}");
            }
            out.push('\n');
        },
        Answer::Typed(t) => {
            out.push_str("Answered in their own words:\n\n");
            out.push_str(&quote(t));
            out.push('\n');
        },
        Answer::Nothing => out.push_str("No option chosen.\n\n"),
    }
    if let Some(n) = &q.notes {
        out.push_str("Notes:\n\n");
        out.push_str(&quote(n));
        out.push('\n');
    }
}

/// The whole file. `c.events` is never empty; the caller writes nothing when
/// there is nothing new.
pub fn render(c: &Capture<'_>) -> String {
    let first = c.events.first().map(|e| e.at).unwrap_or_default();
    let last = c.events.last().map(|e| e.at).unwrap_or_default();
    let mut out = String::from("---\n");
    let _ = writeln!(out, "when: {}", local(first, c.zone, "%Y-%m-%d %H:%M"));
    let _ = writeln!(out, "kind: {}", kind(c.events));
    let _ = writeln!(out, "source: {}", c.session);
    let _ = writeln!(out, "through: {last}");
    if c.bears_on.is_empty() {
        out.push_str("bears_on: []\n");
    } else {
        out.push_str("bears_on:\n");
        for b in c.bears_on {
            let _ = writeln!(out, "  - {b}");
        }
    }
    out.push_str("tags: [chat]\n---\n\n");
    let _ = writeln!(out, "# {}\n", c.title);
    let mut shown: Option<&str> = None;
    for e in c.events {
        if let Some(b) = e.before.as_deref() {
            if shown != Some(b) {
                out.push_str("## What was going on\n\n");
                out.push_str(&aside(b));
                out.push('\n');
                shown = Some(b);
            }
        }
        let at = local(e.at, c.zone, "%H:%M");
        match &e.what {
            Happened::Said(w) => {
                let _ = writeln!(out, "## Said unprompted at {at}\n");
                out.push_str(&quote(w));
                out.push('\n');
            },
            Happened::Answered(qs) => {
                let _ = writeln!(out, "## Asked, answered at {at}\n");
                for q in qs {
                    question(&mut out, q);
                }
            },
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}
