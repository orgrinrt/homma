//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a rule declares about itself, and reading it off its template.
//!
//! A rule is one file, `<name>.md.tmpl`: this meta as frontmatter, then the
//! card, then a marker, then the elaboration. **The card is a prefix of the
//! full rule** rather than a separate document, so nothing is written twice and
//! the two cannot drift.
//!
//! One file rather than two because frontmatter on one of a pair governs the
//! other silently: opening the card template showed none of the meta shaping
//! it, and an elaboration could lose the card beside it and be authored,
//! findable and absent from every session at once.
//!
//! The prefix ordering is what a rule already owes its reader, per
//! `writing-for-agents`: operative content first, complete without the rest.
//!
//! The frontmatter syntax and why it is not YAML are in [`crate::frontmatter`].

use std::fmt;

use serde::Serialize;

use crate::frontmatter::{self, FrontmatterError};

/// Every field a rule may declare.
const KNOWN: &[&str] = &["topics", "fires", "kind", "paths"];

/// What a rule declares about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleMeta {
    /// What the rule is about, in the words a reader arrives with rather than
    /// the words the rule uses. This is what discovery matches on.
    pub topics: Vec<String>,
    /// The moment the rule fires, one line, in the words that moment arrives
    /// in. Required, because a card generated without one is a fact a session
    /// carries and never connects to what it is doing.
    pub fires:  String,
    /// `reflex` for one move with one trigger, `discipline` for a practice
    /// whose parts interact. Decides the card's shape.
    pub kind:   RuleKind,
    /// Path globs the rule is gated on, when it has a file signature. Absent
    /// for the rules that have none, which is most of them.
    pub paths:  Vec<String>,
}

/// Whether a rule is one move or a practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    /// One move with one trigger.
    Reflex,
    /// A practice whose parts interact, keeping its sections.
    Discipline,
}

impl RuleKind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "reflex" => Some(Self::Reflex),
            "discipline" => Some(Self::Discipline),
            _ => None,
        }
    }
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Reflex => "reflex",
            Self::Discipline => "discipline",
        })
    }
}

/// Why a meta block could not be read.
///
/// The frontmatter errors, under the name the rule reader has always used for
/// them, so a caller matching on this does not have to know which layer the
/// refusal came from.
pub type MetaError = FrontmatterError;

/// The line separating the card from the elaboration.
///
/// An HTML comment, so the file still reads as markdown anywhere that renders
/// it and the marker does not appear to a human reading the rendered rule.
pub const ELABORATION_MARKER: &str = "<!-- elaboration -->";

/// The frontmatter block and the body after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub meta: RuleMeta,
    pub body: String,
}

impl Parsed {
    /// Where the marker sits, as the byte range of its own line.
    ///
    /// **A line that is exactly the marker, not the marker anywhere.** A rule
    /// documenting this format has to be able to mention the marker in a
    /// sentence, and the substring form truncated that rule's card at the
    /// mention, silently and in the middle of a word. A structural marker is a
    /// line.
    fn split_at(&self) -> Option<(usize, usize)> {
        let mut offset = 0;
        for line in self.body.split_inclusive('\n') {
            if line.trim() == ELABORATION_MARKER {
                return Some((offset, offset + line.len()));
            }
            offset += line.len();
        }
        None
    }

    /// The card: everything before the elaboration marker.
    ///
    /// A rule with no marker is entirely a card, which is the case for the ones
    /// whose whole statement fits. That is not a defect and is not reported as
    /// one.
    pub fn card(&self) -> &str {
        match self.split_at() {
            Some((start, _)) => self.body[.. start].trim_end(),
            None => self.body.trim_end(),
        }
    }

    /// The elaboration: everything after the marker, empty when there is none.
    pub fn elaboration(&self) -> &str {
        match self.split_at() {
            Some((_, end)) => self.body[end ..].trim_start_matches('\n').trim_end(),
            None => "",
        }
    }

    /// Whether the rule carries an elaboration at all.
    pub fn has_elaboration(&self) -> bool {
        self.split_at().is_some()
    }
}

/// Read a rule's meta and body off the text of its template.
pub fn parse(source: &str) -> Result<Parsed, MetaError> {
    let block = frontmatter::split(source, KNOWN)?;

    let topics = block.required_list("topics")?;
    if topics.is_empty() {
        return Err(MetaError::BadValue {
            key:    "topics".into(),
            reason: "empty, so nothing would ever discover this rule".into(),
        });
    }

    let fires = block.scalar("fires").map_err(|e| {
        match e {
            MetaError::BadValue {
                key,
                ..
            } => {
                MetaError::BadValue {
                    key,
                    reason: "empty, and a card without a trigger never connects to the work".into(),
                }
            },
            other => other,
        }
    })?;

    let raw_kind = block.scalar("kind")?;
    let kind = RuleKind::parse(&raw_kind).ok_or_else(|| {
        MetaError::BadValue {
            key:    "kind".into(),
            reason: format!("`{raw_kind}` is neither `reflex` nor `discipline`"),
        }
    })?;

    let paths = block.list("paths")?;

    Ok(Parsed {
        meta: RuleMeta {
            topics,
            fires,
            kind,
            paths,
        },
        body: block.body,
    })
}

/// How well a rule's topics answer a query, as the count of query terms it
/// matches.
///
/// Substring rather than equality on purpose: somebody asking about "writing"
/// should reach a rule tagged `writing-style`, and one asking about "readme"
/// should reach `readme`. Zero means the rule does not answer at all and is
/// left out rather than ranked last.
pub fn score(meta: &RuleMeta, query: &[String]) -> usize {
    query
        .iter()
        .filter(|term| {
            let term = term.to_lowercase();
            meta.topics.iter().any(|t| {
                let t = t.to_lowercase();
                t.contains(&term) || term.contains(&t)
            })
        })
        .count()
}

/// Split a query as a human types it: commas, or spaces, or both.
pub fn query_terms(raw: &str) -> Vec<String> {
    raw.split([',', ' ', '\t'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
