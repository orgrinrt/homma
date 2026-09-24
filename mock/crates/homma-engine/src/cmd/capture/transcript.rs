//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading a session transcript into the events a capture holds.
//!
//! The transcript is the agent harness's own format, one JSON object per line,
//! and nobody published it as a contract. So a line is read only where it has a
//! shape named here, and a line of such a shape with a field missing or of the
//! wrong kind is refused by line number rather than passed over, since passing
//! over it would drop the person's words without a trace.

use std::collections::HashSet;

use anyhow::{Context, Result, anyhow, bail};
use jiff::Timestamp;
use serde_json::{Map, Value};

/// What the harness writes as the answer when the person wrote notes and picked
/// nothing. Its words, never theirs.
pub const NOTES_ONLY: &str = "(notes only)";

/// One option a question offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label:       String,
    pub description: String,
    pub preview:     Option<String>,
}

/// How the person answered one question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// They picked one or more of the options, named by label.
    Chose(Vec<String>),
    /// They typed something that is not an option's label.
    Typed(String),
    /// They picked nothing, and whatever notes they left stand alone.
    Nothing,
}

/// One question of an ask round, as it was put and as it was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub text:    String,
    pub header:  String,
    pub options: Vec<Choice>,
    pub answer:  Answer,
    pub notes:   Option<String>,
}

/// What happened, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happened {
    /// Words the person typed, whether as a turn of their own or into a
    /// running one.
    Said(String),
    /// A round of questions put to them, with their answers.
    Answered(Vec<Question>),
}

/// One thing the person said or answered, with the agent's last text before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub at:     Timestamp,
    pub what:   Happened,
    /// The last text the agent wrote before this, verbatim.
    ///
    /// FIXME: the context is the agent's own words copied whole; a summary
    /// through muisti's `answer` replaces it once muisti builds where homma
    /// runs, which today is only the lifebook.
    pub before: Option<String>,
}

impl Event {
    /// The person's own words in this event, in order: what they said, their
    /// typed answers and their notes. What a path is looked for in.
    pub fn words(&self) -> Vec<&str> {
        match &self.what {
            Happened::Said(w) => vec![w.as_str()],
            Happened::Answered(qs) => {
                let mut out = Vec::new();
                for q in qs {
                    if let Answer::Typed(t) = &q.answer {
                        out.push(t.as_str());
                    }
                    if let Some(n) = &q.notes {
                        out.push(n.as_str());
                    }
                }
                out
            },
        }
    }
}

/// Every event in `text`, a whole transcript, in the order the lines give them.
pub fn read(text: &str) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut seen = HashSet::new();
    let mut before: Option<String> = None;
    for (at, line) in text.lines().enumerate() {
        let n = at + 1;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .with_context(|| format!("transcript line {n} is not JSON"))?;
        let Some(o) = v.as_object() else {
            bail!("transcript line {n} is not an object");
        };
        // A sub-agent's lines are its own conversation, not this one.
        if o.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        // A resumed session can replay a line; its uuid says so.
        if let Some(uuid) = o.get("uuid").and_then(Value::as_str) {
            if !seen.insert(uuid.to_string()) {
                continue;
            }
        }
        let happened = match o.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                if let Some(t) = last_text(o) {
                    before = Some(t);
                }
                None
            },
            Some("user") => user(o).with_context(|| format!("transcript line {n}"))?,
            Some("attachment") => queued(o).with_context(|| format!("transcript line {n}"))?,
            _ => None,
        };
        if let Some(what) = happened {
            events.push(Event {
                at: stamp(o).with_context(|| format!("transcript line {n}"))?,
                what,
                before: before.clone(),
            });
        }
    }
    Ok(events)
}

fn stamp(o: &Map<String, Value>) -> Result<Timestamp> {
    let s = o
        .get("timestamp")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("no `timestamp`"))?;
    s.parse()
        .with_context(|| format!("`timestamp` {s:?} is not an instant"))
}

fn human(v: Option<&Value>) -> bool {
    v.and_then(|o| o.get("kind")).and_then(Value::as_str) == Some("human")
}

/// The last text block of an assistant line, if it has one.
fn last_text(o: &Map<String, Value>) -> Option<String> {
    o.get("message")?
        .get("content")?
        .as_array()?
        .iter()
        .rev()
        .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|b| b.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn user(o: &Map<String, Value>) -> Result<Option<Happened>> {
    if let Some(r) = o
        .get("toolUseResult")
        .and_then(Value::as_object)
        .filter(|r| r.contains_key("answers"))
    {
        return round(r).map(|q| Some(Happened::Answered(q)));
    }
    if !human(o.get("origin")) {
        return Ok(None);
    }
    let content = o
        .get("message")
        .and_then(|m| m.get("content"))
        .ok_or_else(|| anyhow!("a typed line with no `message.content`"))?;
    match content {
        Value::String(s) => Ok(Some(Happened::Said(s.clone()))),
        // Text typed beside an image arrives as blocks. The image is not
        // words, and the text blocks are kept as they were typed.
        Value::Array(blocks) => {
            let text: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Happened::Said(text.join("\n\n"))))
            }
        },
        _ => bail!("a typed line whose `message.content` is neither text nor blocks"),
    }
}

fn queued(o: &Map<String, Value>) -> Result<Option<Happened>> {
    let Some(a) = o.get("attachment").and_then(Value::as_object) else {
        return Ok(None);
    };
    if a.get("type").and_then(Value::as_str) != Some("queued_command") || !human(a.get("origin")) {
        return Ok(None);
    }
    let words = a
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("a queued message with no `prompt` text"))?;
    Ok(Some(Happened::Said(words.to_string())))
}

fn string(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no `{key}`"))
}

fn round(r: &Map<String, Value>) -> Result<Vec<Question>> {
    let questions = r
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("an answered round with no `questions`"))?;
    let answers = r
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("`answers` is not a map"))?;
    let notes = r.get("annotations").and_then(Value::as_object);
    questions
        .iter()
        .map(|q| {
            let text = string(q, "question")?;
            let header = string(q, "header")?;
            let options = q
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("question {text:?} has no `options`"))?
                .iter()
                .map(|c| {
                    Ok(Choice {
                        label:       string(c, "label")?,
                        description: string(c, "description")?,
                        preview:     c.get("preview").and_then(Value::as_str).map(str::to_string),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let answer = match answers.get(&text) {
                None => Answer::Nothing,
                Some(a) => {
                    let a = a
                        .as_str()
                        .ok_or_else(|| anyhow!("the answer to {text:?} is not text"))?;
                    answered(a, &options)
                },
            };
            let notes = notes
                .and_then(|n| n.get(&text))
                .and_then(|n| n.get("notes"))
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .map(str::to_string);
            Ok(Question {
                text,
                header,
                options,
                answer,
                notes,
            })
        })
        .collect()
}

/// Which of the three an answer string is.
pub fn answered(a: &str, options: &[Choice]) -> Answer {
    if a == NOTES_ONLY {
        return Answer::Nothing;
    }
    let is_label = |s: &str| options.iter().any(|c| c.label == s);
    if is_label(a) {
        return Answer::Chose(vec![a.to_string()]);
    }
    let parts: Vec<&str> = a.split(", ").collect();
    if parts.len() > 1 && parts.iter().all(|p| is_label(p)) {
        return Answer::Chose(parts.into_iter().map(str::to_string).collect());
    }
    Answer::Typed(a.to_string())
}
