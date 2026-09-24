//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Lines a tool typed into the session's terminal for the person.
//!
//! The harness marks such a line human exactly as it marks the person's own, so
//! the tool writes down what it typed beside the transcript and a capture takes
//! those lines back out.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use jiff::Timestamp;
use serde_json::Value;

use super::transcript::{Event, Happened};

/// One line a tool typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    /// The instant just before it typed.
    pub at:   Timestamp,
    /// Exactly what it typed.
    pub text: String,
}

/// Where the tools' record for a transcript sits: beside it, named for the
/// session. Not a `.jsonl`, so never taken for a transcript.
pub fn beside(transcript: &Path) -> PathBuf {
    let stem = transcript
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    transcript.with_file_name(format!("{stem}.typed-by-tools.ndjson"))
}

/// Every record in `text`, one JSON object a line, in the order written.
pub fn records(text: &str) -> Result<Vec<Typed>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .with_context(|| format!("typed-by-tools line {n} is not JSON"))?;
        let at = v
            .get("at")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("typed-by-tools line {n} has no `at`"))?;
        let at: Timestamp = at
            .parse()
            .with_context(|| format!("typed-by-tools line {n}: `at` {at:?} is not an instant"))?;
        let text = v
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("typed-by-tools line {n} has no `text`"))?
            .to_string();
        out.push(Typed {
            at,
            text,
        });
    }
    Ok(out)
}

/// The records beside `transcript`, none when there is no such file.
pub fn read(transcript: &Path) -> Result<Vec<Typed>> {
    let path = beside(transcript);
    match std::fs::read_to_string(&path) {
        Ok(text) => records(&text).with_context(|| format!("reading {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// `words` as typed, or with one wrapper the harness puts round a paste taken
/// off: `<pasted_content id="N">`, a newline, the text, a newline, and
/// `</pasted_content id="N">`.
fn unwrapped(words: &str) -> Option<&str> {
    let rest = words.strip_prefix("<pasted_content id=\"")?;
    let (id, rest) = rest.split_once("\">\n")?;
    let inner = rest.strip_suffix(&format!("\n</pasted_content id=\"{id}\">"))?;
    Some(inner)
}

fn is(words: &str, text: &str) -> bool {
    words == text || unwrapped(words) == Some(text)
}

/// `events` without the said ones a tool typed: each record takes the first
/// said event, in transcript order and not yet taken, whose words are its text
/// and whose stamp is at or after its `at`.
pub fn without(events: Vec<Event>, records: &[Typed]) -> Vec<Event> {
    let mut taken = vec![false; events.len()];
    for r in records {
        let hit = events.iter().enumerate().position(|(i, e)| {
            !taken[i] && e.at >= r.at && matches!(&e.what, Happened::Said(w) if is(w, &r.text))
        });
        if let Some(i) = hit {
            taken[i] = true;
        }
    }
    events
        .into_iter()
        .zip(taken)
        .filter_map(|(e, t)| (!t).then_some(e))
        .collect()
}
