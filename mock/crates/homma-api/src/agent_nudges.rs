//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The nudges table: rows under `[agent.nudges]` in `homma.toml`, each saying
//! what agent call or event it takes, what it says, and whether it informs,
//! stops a call once, or refuses it. No I/O; `homma-engine` runs the rows.

use std::collections::BTreeSet;
use std::fmt;

use regex::Regex;

use crate::agent_hooks::{AGENT_EVENTS, AgentHook, EVERY_TOOL, InvalidAgentHooks};

/// The only event a call can be stopped on.
pub const STOPPING_EVENT: &str = "PreToolUse";

/// What a row does when it applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    /// Said as added context, again only once its time to live has passed.
    Inform,
    /// The call is stopped until the stop is answered.
    Once,
    /// The call is stopped every time.
    Refuse,
}

impl Strength {
    /// Whether this strength stops the call it applies to.
    pub const fn stops(self) -> bool {
        matches!(self, Strength::Once | Strength::Refuse)
    }
}

/// A time to live, in whole seconds, written `90s`, `30m` or `2h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ttl(u64);

impl Ttl {
    /// One hour, which a row naming no `ttl` gets.
    pub const HOUR: Ttl = Ttl(3600);

    /// Read `<count><unit>`, the unit one of `s`, `m` and `h` and the count
    /// above zero.
    pub fn parse(text: &str) -> Result<Ttl, InvalidNudge> {
        let bad = || InvalidNudge::Ttl(text.to_string());
        let unit = text.chars().last().ok_or_else(bad)?;
        let scale = match unit {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            _ => return Err(bad()),
        };
        let count = &text[.. text.len() - 1];
        if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let count: u64 = count.parse().map_err(|_| bad())?;
        match count.checked_mul(scale) {
            Some(0) | None => Err(bad()),
            Some(s) => Ok(Ttl(s)),
        }
    }

    /// The span in seconds.
    pub const fn seconds(self) -> u64 {
        self.0
    }
}

/// A row as written, before it is checked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NudgeRow {
    pub name:     String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event:    Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools:    Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches:  Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when:     Option<String>,
    pub says:     String,
    pub strength: Strength,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer:   Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl:      Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub brief:    bool,
}

/// One checked row. Built through [`Nudge::new`] and nowhere else, so a row
/// whose expressions do not compile, whose programs leave the workspace, or
/// which stops a call on an event that cannot stop one does not exist.
#[derive(Debug, Clone)]
pub struct Nudge {
    row:     NudgeRow,
    event:   String,
    tools:   Option<Regex>,
    matches: Option<Regex>,
    ttl:     Ttl,
}

impl Nudge {
    /// A row, or why it cannot be one.
    pub fn new(row: NudgeRow) -> Result<Nudge, InvalidNudge> {
        let name = row.name.clone();
        let fault = |what: Fault| {
            InvalidNudge::Row {
                name: name.clone(),
                what,
            }
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(InvalidNudge::Name(name));
        }
        let event = row
            .event
            .clone()
            .unwrap_or_else(|| STOPPING_EVENT.to_string());
        if !AGENT_EVENTS.contains(&event.as_str()) {
            return Err(fault(Fault::Event(event)));
        }
        if row.strength.stops() && event != STOPPING_EVENT {
            return Err(fault(Fault::CannotStop(event)));
        }
        let tools = match row.tools.as_deref().map(str::trim) {
            None | Some("") | Some(EVERY_TOOL) => None,
            Some(t) => {
                Some(
                    Regex::new(&format!("^(?:{t})$"))
                        .map_err(|e| fault(Fault::Expression("tools", e.to_string())))?,
                )
            },
        };
        let matches = match row.matches.as_deref() {
            None => None,
            Some(m) => {
                Some(
                    Regex::new(m)
                        .map_err(|e| fault(Fault::Expression("matches", e.to_string())))?,
                )
            },
        };
        for (key, path) in [("when", &row.when), ("evidence", &row.evidence)] {
            if let Some(p) = path {
                AgentHook::new("", p.clone(), Vec::new())
                    .map_err(|e| fault(Fault::Program(key, e)))?;
            }
        }
        if row.says.trim().is_empty() {
            return Err(fault(Fault::Silent));
        }
        if let Some(a) = &row.answer {
            if row.strength != Strength::Once {
                return Err(fault(Fault::Unread("answer")));
            }
            if a.is_empty() || a.chars().any(char::is_whitespace) {
                return Err(fault(Fault::Answer(a.clone())));
            }
        }
        if row.evidence.is_some() && row.answer.is_none() {
            return Err(fault(Fault::Unread("evidence")));
        }
        if row.brief && row.strength != Strength::Inform {
            return Err(fault(Fault::Unread("brief")));
        }
        let ttl = match &row.ttl {
            None => Ttl::HOUR,
            Some(t) => Ttl::parse(t)?,
        };
        Ok(Nudge {
            row,
            event,
            tools,
            matches,
            ttl,
        })
    }

    pub fn name(&self) -> &str {
        &self.row.name
    }

    /// The host event the row runs on.
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The row's tools as the host's matcher, every tool where it names none.
    pub fn matcher(&self) -> &str {
        if self.tools.is_some() {
            self.row.tools.as_deref().map_or(EVERY_TOOL, str::trim)
        } else {
            EVERY_TOOL
        }
    }

    /// Whether the row takes a call of `tool`.
    pub fn takes_tool(&self, tool: &str) -> bool {
        self.tools.as_ref().is_none_or(|r| r.is_match(tool))
    }

    /// Whether the row takes a call whose text is `text`.
    pub fn takes_text(&self, text: &str) -> bool {
        self.matches.as_ref().is_none_or(|r| r.is_match(text))
    }

    /// The predicate program, a path under the workspace.
    pub fn when(&self) -> Option<&str> {
        self.row.when.as_deref()
    }

    pub fn says(&self) -> &str {
        &self.row.says
    }

    pub fn strength(&self) -> Strength {
        self.row.strength
    }

    /// The marker answering a stop, for a `once` row that names one.
    pub fn answer(&self) -> Option<&str> {
        self.row.answer.as_deref()
    }

    /// The program run before an answer is taken, a path under the workspace.
    pub fn evidence(&self) -> Option<&str> {
        self.row.evidence.as_deref()
    }

    pub fn ttl(&self) -> Ttl {
        self.ttl
    }

    /// Whether the row says the brief of the call's text after its `says`.
    pub fn brief(&self) -> bool {
        self.row.brief
    }

    /// The row as written.
    pub fn row(&self) -> &NudgeRow {
        &self.row
    }
}

impl PartialEq for Nudge {
    fn eq(&self, other: &Nudge) -> bool {
        self.row == other.row
    }
}

impl Eq for Nudge {}

impl serde::Serialize for Nudge {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.row.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for Nudge {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Nudge::new(NudgeRow::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// The rows, in the order written, every name once. `[agent.nudges]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Nudges(Vec<Nudge>);

impl Nudges {
    /// The rows, or the name two of them share.
    pub fn new(rows: Vec<Nudge>) -> Result<Nudges, InvalidNudge> {
        let mut out = Nudges::default();
        for row in rows {
            out.push(row)?;
        }
        Ok(out)
    }

    fn push(&mut self, row: Nudge) -> Result<(), InvalidNudge> {
        if self.0.iter().any(|r| r.name() == row.name()) {
            return Err(InvalidNudge::Twice(row.name().to_string()));
        }
        self.0.push(row);
        Ok(())
    }

    /// Add another layer's rows after these, in that layer's order. A name
    /// already here refuses the layer, and nothing of it is added.
    pub fn append(&mut self, layer: &Nudges) -> Result<(), InvalidNudge> {
        let mut next = self.clone();
        for row in &layer.0 {
            next.push(row.clone())?;
        }
        *self = next;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Nudge> {
        self.0.iter()
    }

    /// The rows of one event, in order.
    pub fn of_event<'a>(&'a self, event: &'a str) -> impl Iterator<Item = &'a Nudge> + 'a {
        self.0.iter().filter(move |r| r.event() == event)
    }

    /// Every event some row names.
    pub fn events(&self) -> BTreeSet<&str> {
        self.0.iter().map(Nudge::event).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'de> serde::Deserialize<'de> for Nudges {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Nudges::new(Vec::<Nudge>::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// What is wrong with one key of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// `event` names no host event.
    Event(String),
    /// A stopping strength on an event that cannot stop a call.
    CannotStop(String),
    /// `tools` or `matches` is not a regular expression.
    Expression(&'static str, String),
    /// `when` or `evidence` is not a path under the workspace.
    Program(&'static str, InvalidAgentHooks),
    /// `says` is blank.
    Silent,
    /// A key the row's strength never reads.
    Unread(&'static str),
    /// `answer` is blank or carries whitespace.
    Answer(String),
}

/// Why `[agent.nudges]`, or one row of it, is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidNudge {
    /// A name that is not a plain slug.
    Name(String),
    /// Two rows of one name.
    Twice(String),
    /// A `ttl` that is not a duration.
    Ttl(String),
    Row {
        name: String,
        what: Fault,
    },
}

impl fmt::Display for InvalidNudge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidNudge::Name(n) => {
                write!(
                    f,
                    "a nudge is named `{n}`; a name is lowercase letters, digits, `-` and `_`"
                )
            },
            InvalidNudge::Twice(n) => write!(f, "two nudges are named `{n}`; a name is used once"),
            InvalidNudge::Ttl(t) => {
                write!(
                    f,
                    "a nudge's `ttl` is `{t}`; it is a count above zero with `s`, `m` or `h`, as \
                     `90s`, `30m` or `2h`"
                )
            },
            InvalidNudge::Row {
                name,
                what,
            } => {
                write!(f, "the nudge `{name}` ")?;
                match what {
                    Fault::Event(e) => write!(f, "runs on `{e}`, which the host has no hook for"),
                    Fault::CannotStop(e) => {
                        write!(
                            f,
                            "stops a call on `{e}`, and only `{STOPPING_EVENT}` can stop one; \
                             make it `inform` or move it to `{STOPPING_EVENT}`"
                        )
                    },
                    Fault::Expression(key, e) => {
                        write!(f, "has a `{key}` that is not a regular expression: {e}")
                    },
                    Fault::Program(key, e) => write!(f, "has a `{key}` that cannot run: {e}"),
                    Fault::Silent => write!(f, "says nothing; `says` is what the agent is told"),
                    Fault::Unread(key) => {
                        write!(
                            f,
                            "sets `{key}`, which nothing reads for its strength: `answer` is for \
                             `once`, `evidence` for a row with an `answer`, and `brief` for \
                             `inform`"
                        )
                    },
                    Fault::Answer(a) => {
                        write!(
                            f,
                            "is answered by `{a}`; a marker is one word with no space in it"
                        )
                    },
                }
            },
        }
    }
}

impl std::error::Error for InvalidNudge {}

#[cfg(test)]
#[path = "agent_nudges_tests.rs"]
mod tests;
