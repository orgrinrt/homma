//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which tool results are an ask round, and the ones whose question is missing.
//!
//! A result is a round when it names a call to `AskUserQuestion` that an earlier
//! line holds. A result naming the call of any other tool is not read. A result
//! naming a call that no line holds is what a cut write leaves of the line that
//! carried the question, and the answer in it is the person's words, so it is
//! read by its own shape where it has a round's, and said where it has the keys
//! of one and is not.

use std::collections::HashSet;
use std::fmt;

use serde_json::{Map, Value};

/// The tool the agent calls to put questions to the person.
pub const ASK: &str = "AskUserQuestion";

/// Every call the agent made, by the id its result names.
#[derive(Debug, Default)]
pub struct Calls {
    all:  HashSet<String>,
    asks: HashSet<String>,
}

/// What the tool results on a user line answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answering {
    /// A call to `AskUserQuestion` that an earlier line holds.
    Ask,
    /// A call to some other tool, or no tool result at all.
    Other,
    /// A call that no earlier line holds.
    Unknown,
}

/// A tool result whose call no earlier line holds, and whether it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unasked {
    /// The line's number counted from 1, as the refusals count it.
    pub line: usize,
    /// Whether its own shape let it be read as a round.
    pub read: bool,
}

impl fmt::Display for Unasked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let how = if self.read {
            "read as a round by its own shape"
        } else {
            "and it could not be read as a round, so what it says is not in the capture"
        };
        write!(
            f,
            "transcript line {} carries the result of a question the transcript does not hold, \
             {how}",
            self.line
        )
    }
}

fn blocks(o: &Map<String, Value>) -> &[Value] {
    o.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn is(b: &Value, kind: &str) -> bool {
    b.get("type").and_then(Value::as_str) == Some(kind)
}

impl Calls {
    /// Notes the calls on an assistant line.
    pub fn note(&mut self, o: &Map<String, Value>) {
        for b in blocks(o).iter().filter(|b| is(b, "tool_use")) {
            let Some(id) = b.get("id").and_then(Value::as_str) else {
                continue;
            };
            self.all.insert(id.to_string());
            if b.get("name").and_then(Value::as_str) == Some(ASK) {
                self.asks.insert(id.to_string());
            }
        }
    }

    /// What the tool results on a user line answer.
    pub fn answering(&self, o: &Map<String, Value>) -> Answering {
        let ids: Vec<&str> = blocks(o)
            .iter()
            .filter(|b| is(b, "tool_result"))
            .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
            .collect();
        if ids.iter().any(|i| self.asks.contains(*i)) {
            Answering::Ask
        } else if !ids.is_empty() && !ids.iter().any(|i| self.all.contains(*i)) {
            Answering::Unknown
        } else {
            Answering::Other
        }
    }
}

/// Whether a `toolUseResult` carries the keys of a round, which is all that
/// tells an ask's result from another tool's when the call is missing.
pub fn shaped(result: &Map<String, Value>) -> bool {
    result.contains_key("questions") || result.contains_key("answers")
}
