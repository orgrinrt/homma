//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The capture an answers run writes, in the shape the dashboard's drain has always written by
//! hand: the question and every option in an indented fence, the choice in plain words under
//! `Chosen:`, what was written as a blockquote, and the store key of each document listed in the
//! front matter, which is what makes a second run take nothing.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use super::Source;
use super::docs::{Collection, Document};

/// A fence one backtick longer than any run inside `text`, three at least.
fn fence(text: &str) -> String {
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
    "`".repeat((longest + 1).max(3))
}

fn indented(text: &str) -> String {
    text.split('\n')
        .map(|l| if l.is_empty() { String::new() } else { format!("  {l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quoted(text: &str) -> String {
    text.split('\n')
        .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block(text: &str) -> String {
    let f = fence(text);
    format!("  {f}text\n{}\n  {f}", indented(text))
}

fn answer_entry(d: &Document) -> String {
    let mut out = format!(
        "## {}, answered at {}\n\n{}\n\n",
        d.label(),
        d.at,
        block(d.text("asks"))
    );
    let options: Vec<&str> = d.body["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o.as_str())
        .collect();
    if options.is_empty() {
        out.push_str("Options offered: none, free text.\n");
    } else {
        out.push_str("Options offered:\n\n");
        for (i, o) in options.iter().enumerate() {
            let _ = writeln!(out, "{}. {o}", i + 1);
        }
        out.push_str("\nChosen:\n\n");
        let chosen: Vec<&str> = match &d.body["choice"] {
            serde_json::Value::Array(a) => a.iter().filter_map(|c| c.as_str()).collect(),
            serde_json::Value::String(s) => vec![s.as_str()],
            _ => Vec::new(),
        };
        if chosen.is_empty() {
            out.push_str("- none of the options, free text\n");
        }
        for c in chosen {
            let _ = writeln!(out, "- {c}");
        }
    }
    if !d.text("text").is_empty() {
        let _ = write!(out, "\n{}\n", quoted(d.text("text")));
    }
    out
}

fn note_entry(d: &Document) -> String {
    format!(
        "## A note, left at {}\n\n{}\n",
        d.at,
        quoted(d.text("text"))
    )
}

/// What the capture is called, from the questions answered and the notes left.
pub fn title(docs: &[Document], source: &Source<'_>) -> String {
    let who = source.speaker();
    let keys: Vec<String> = docs
        .iter()
        .filter(|d| d.collection == Collection::Answers)
        .map(Document::label)
        .collect();
    let notes = docs.len() - keys.len();
    let answered = match keys.as_slice() {
        [] => String::new(),
        [a] => format!("{who} answers {a}"),
        [a, b] => format!("{who} answers {a} and {b}"),
        [a, b, rest @ ..] => format!("{who} answers {a}, {b} and {} more", rest.len()),
    };
    let left = match notes {
        0 => String::new(),
        1 => "a note".into(),
        n => format!("{n} notes"),
    };
    match (answered.is_empty(), left.is_empty()) {
        (true, _) => format!("{who} leaves {left}{}", source.place()),
        (_, true) => answered,
        _ => format!("{answered} and leaves {left}"),
    }
}

/// The capture's text. `docs` are in the order they are to be written; `when` is
/// `YYYY-MM-DD HH:MM` of the latest, and `source` says where the words were left.
pub fn text(docs: &[Document], when: &str, source: &Source<'_>, title: &str) -> String {
    let answers = docs.iter().any(|d| d.collection == Collection::Answers);
    let notes = docs.iter().any(|d| d.collection == Collection::Notes);
    let (kind, tags) = match (answers, notes) {
        (true, false) => ("answers", "answers"),
        (false, _) => ("notes", "notes"),
        (true, true) => ("mixed", "answers, notes"),
    };
    let bears: BTreeSet<&str> = docs
        .iter()
        .filter(|d| d.collection == Collection::Answers)
        .map(|d| d.text("where"))
        .collect();
    let mut out = format!(
        "---\nwhen: {when}\nkind: {kind}\nsource: {}\nbears_on:",
        source.words()
    );
    if bears.is_empty() {
        out.push_str(" []");
    } else {
        for b in &bears {
            let _ = write!(out, "\n  - {b}");
        }
    }
    let _ = write!(out, "\ntags: [{}, {tags}]\nstore:\n", source.tag());
    out.push_str(
        &docs
            .iter()
            .map(|d| format!("  - {}", d.key))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let _ = write!(out, "\n---\n\n# {title}\n\n{}\n\n", source.lead());
    out.push_str(
        &docs
            .iter()
            .map(|d| {
                if d.collection == Collection::Answers {
                    answer_entry(d)
                } else {
                    note_entry(d)
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // The drain ends a capture on a blank line, and a second run of either must agree.
    out.push('\n');
    out
}

/// `YYYY-MM-DD HH:MM` from an RFC 3339 timestamp.
pub fn when_of(at: &str) -> String {
    format!(
        "{} {}",
        at.get(.. 10).unwrap_or_default(),
        at.get(11 .. 16).unwrap_or_default()
    )
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    let cut: String = trimmed.chars().take(60).collect();
    cut.trim_end_matches('-').to_string()
}

/// 32-bit FNV-1a of `text`, in hex.
pub fn digest(text: &str) -> String {
    let mut h: u32 = 0x811C_9DC5;
    for b in text.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

/// The digest of the sorted keys: the suffix that tells two captures of one minute apart.
fn short_hash(docs: &[Document]) -> String {
    let mut keys: Vec<&str> = docs.iter().map(|d| d.key.as_str()).collect();
    keys.sort_unstable();
    digest(&keys.join("\n"))
}

/// The file a capture gets: the minute of the latest `at`, then the title as a slug. A name
/// already taken in `store` gets the hash of the keys after it.
pub fn name(store: &Path, latest: &str, title: &str, docs: &[Document]) -> String {
    let stamp: String = latest
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect();
    let plain = format!("{stamp}_{}.md", slug(title));
    if store.join(&plain).exists() {
        format!("{stamp}_{}-{}.md", slug(title), short_hash(docs))
    } else {
        plain
    }
}

/// The store keys the captures in `store` already list, read between each file's first two
/// fences only: a line in a capture's body that reads like a key is somebody's text.
pub fn recorded(store: &Path) -> Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    if !store.is_dir() {
        return Ok(keys);
    }
    for entry in std::fs::read_dir(store).with_context(|| format!("reading {}", store.display()))? {
        let path = entry?.path();
        if path.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut lines = text.lines();
        if lines.next() != Some("---") {
            continue;
        }
        for line in lines.take_while(|l| *l != "---") {
            let key = line
                .strip_prefix("  - ")
                .filter(|k| k.starts_with("answers/") || k.starts_with("notes/"));
            if let Some(k) = key {
                keys.insert(k.to_string());
            }
        }
    }
    Ok(keys)
}
