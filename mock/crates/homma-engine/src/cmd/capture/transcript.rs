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

use std::collections::{HashMap, HashSet};

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

/// How the person answered one question: the options they picked, named by
/// label, and whatever they typed after them. Both empty is an answer too, the
/// one where only the notes say anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub chose: Vec<String>,
    pub typed: Option<String>,
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
    /// Where in the transcript it was written, counted from 0. The order a
    /// capture follows, since the stamps are not in it.
    pub line:   usize,
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
                    if let Some(t) = &q.answer.typed {
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

/// A transcript read whole.
#[derive(Debug, Default)]
pub struct Transcript {
    /// Every event, in the order the lines give them.
    pub events: Vec<Event>,
    /// The line each `uuid` was first written on, which is what a watermark
    /// is looked up in.
    pub lines:  HashMap<String, usize>,
    /// The `uuid` of the last complete line carrying one, the watermark a
    /// capture of all of this records.
    pub last:   Option<String>,
}

impl Transcript {
    /// Where the line carrying `uuid` sits, if the transcript has it.
    pub fn line_of(&self, uuid: &str) -> Option<usize> {
        self.lines.get(uuid).copied()
    }
}

/// Every event in `text`, a whole transcript.
///
/// A last line with no newline is still being written and is not read. A
/// message the harness wrote more than once, as a queued-command attachment and
/// a typed line, or as two attachments, is tied by `source_uuid` and read where
/// it was written first.
pub fn read(text: &str) -> Result<Transcript> {
    let complete = text.rfind('\n').map_or("", |i| &text[..= i]);
    let mut out = Transcript::default();
    let mut before: Option<String> = None;
    // The message each human line is a copy of: a typed line's own uuid, or
    // the uuid a queued attachment names as `source_uuid`. A key met again is
    // a copy, and is not read.
    let mut messages: HashSet<String> = HashSet::new();
    // The ids of the calls the agent made to ask the person something. A tool
    // result is a round only when it answers one of these.
    let mut asks: HashSet<String> = HashSet::new();
    for (at, line) in complete.lines().enumerate() {
        let n = at + 1;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)
            .with_context(|| format!("transcript line {n} is not JSON"))?;
        let Some(o) = v.as_object() else {
            bail!("transcript line {n} is not an object");
        };
        let uuid = o.get("uuid").and_then(Value::as_str);
        // A resumed session can replay a line; its uuid says so.
        if let Some(u) = uuid {
            if out.lines.contains_key(u) {
                continue;
            }
            out.lines.insert(u.to_string(), at);
            out.last = Some(u.to_string());
        }
        // A sub-agent's lines are its own conversation, not this one.
        if o.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let happened = match o.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                if let Some(t) = last_text(o) {
                    before = Some(t);
                }
                asks.extend(ask_calls(o));
                None
            },
            Some("user") => {
                let said = user(o, &asks).with_context(|| format!("transcript line {n}"))?;
                if let (Some(Happened::Said(_)), Some(u)) = (&said, uuid) {
                    if !messages.insert(u.to_string()) {
                        continue;
                    }
                }
                said
            },
            Some("attachment") => {
                let said = queued(o).with_context(|| format!("transcript line {n}"))?;
                if said.is_some() {
                    if let Some(s) = twin(o) {
                        if !messages.insert(s.to_string()) {
                            continue;
                        }
                    }
                }
                said
            },
            _ => None,
        };
        if let Some(what) = happened {
            out.events.push(Event {
                line: at,
                at: stamp(o).with_context(|| format!("transcript line {n}"))?,
                what,
                before: before.clone(),
            });
        }
    }
    Ok(out)
}

/// The `uuid` a queued attachment's typed twin would carry.
fn twin(o: &Map<String, Value>) -> Option<&str> {
    o.get("attachment")?.get("source_uuid")?.as_str()
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

/// The tool the agent calls to put questions to the person.
pub const ASK: &str = "AskUserQuestion";

/// The ids of the ask calls on an assistant line.
fn ask_calls(o: &Map<String, Value>) -> Vec<String> {
    let Some(blocks) = o
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter(|b| b.get("name").and_then(Value::as_str) == Some(ASK))
        .filter_map(|b| b.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// Whether a user line carries the result of one of `asks`.
fn answers_an_ask(o: &Map<String, Value>, asks: &HashSet<String>) -> bool {
    o.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b.get("type").and_then(Value::as_str) == Some("tool_result")
                    && b.get("tool_use_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| asks.contains(id))
            })
        })
}

fn user(o: &Map<String, Value>, asks: &HashSet<String>) -> Result<Option<Happened>> {
    if answers_an_ask(o, asks) {
        // A rejected ask carries a string here rather than a round, and says
        // nothing of the person's.
        return match o.get("toolUseResult").and_then(Value::as_object) {
            Some(r) => round(r).map(|q| Some(Happened::Answered(q))),
            None => Ok(None),
        };
    }
    if !human(o.get("origin")) {
        return Ok(None);
    }
    let content = o
        .get("message")
        .and_then(|m| m.get("content"))
        .ok_or_else(|| anyhow!("a typed line with no `message.content`"))?;
    words(content)
        .map(|w| w.map(Happened::Said))
        .context("a typed line whose `message.content` is neither text nor blocks")
}

/// The person's words, a string or a list of content blocks.
///
/// Text typed beside an image arrives as blocks. The image is not words, and
/// the text blocks are kept as they were typed, joined by a blank line; blocks
/// with no text in them are nothing said.
fn words(content: &Value) -> Result<Option<String>> {
    match content {
        Value::String(s) => Ok(Some(s.clone())),
        Value::Array(blocks) => {
            let text: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            Ok((!text.is_empty()).then(|| text.join("\n\n")))
        },
        _ => bail!("words are neither text nor blocks"),
    }
}

fn queued(o: &Map<String, Value>) -> Result<Option<Happened>> {
    let Some(a) = o.get("attachment").and_then(Value::as_object) else {
        return Ok(None);
    };
    if a.get("type").and_then(Value::as_str) != Some("queued_command") || !human(a.get("origin")) {
        return Ok(None);
    }
    let prompt = a
        .get("prompt")
        .ok_or_else(|| anyhow!("a queued message with no `prompt`"))?;
    words(prompt)
        .map(|w| w.map(Happened::Said))
        .context("a queued message whose `prompt` is neither text nor blocks")
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
            let takes = match q.get("multiSelect").and_then(Value::as_bool) {
                Some(true) => Takes::Several,
                Some(false) => Takes::One,
                None => bail!("question {text:?} has no `multiSelect`"),
            };
            let answer = match answers.get(&text) {
                None => Answer::default(),
                Some(a) => {
                    let a = a
                        .as_str()
                        .ok_or_else(|| anyhow!("the answer to {text:?} is not text"))?;
                    answered(a, &options, takes)
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

/// How many options a question lets the person pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    One,
    Several,
}

/// An answer string split into the labels picked and what was typed after them.
///
/// On a question taking one answer, the answer is a label when it equals one
/// and typed text whole otherwise, even where it starts with a label. On one
/// taking several, the harness joins picked labels with `, `, puts a label
/// holding `, ` in double quotes, and appends the free text after the labels.
/// So labels are taken off the front one at a time, the longest that fits
/// first, each ending at `, ` or at the end, and what is left is the typed part.
pub fn answered(a: &str, options: &[Choice], takes: Takes) -> Answer {
    if a == NOTES_ONLY || a.is_empty() {
        return Answer::default();
    }
    if takes == Takes::One {
        return if options.iter().any(|c| c.label == a) {
            Answer {
                chose: vec![a.to_string()],
                typed: None,
            }
        } else {
            Answer {
                chose: Vec::new(),
                typed: Some(a.to_string()),
            }
        };
    }
    let mut labels: Vec<&str> = options.iter().map(|c| c.label.as_str()).collect();
    labels.sort_by_key(|l| std::cmp::Reverse(l.len()));
    let mut chose = Vec::new();
    let mut rest = a;
    while !rest.is_empty() {
        fn ends(after: &str) -> Option<&str> {
            if after.is_empty() { Some(after) } else { after.strip_prefix(", ") }
        }
        let next = labels.iter().copied().find_map(|l| {
            let quoted = rest
                .strip_prefix('"')
                .and_then(|r| r.strip_prefix(l))
                .and_then(|r| r.strip_prefix('"'))
                .and_then(ends);
            quoted
                .or_else(|| rest.strip_prefix(l).and_then(ends))
                .map(|after| (l, after))
        });
        let Some((label, after)) = next else {
            break;
        };
        chose.push(label.to_string());
        rest = after;
    }
    Answer {
        chose,
        typed: (!rest.is_empty()).then(|| rest.to_string()),
    }
}
