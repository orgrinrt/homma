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
//! # Why the reader is strict rather than a YAML parser
//!
//! The meta is `key: scalar` and `key: [a, b, c]`, and nothing else. A full
//! parser would accept nested maps, anchors, block scalars and multi-document
//! streams, none of which the generation pass can render or round-trip, so
//! every one of them is a file that parses and then produces something nobody
//! asked for. This refuses what it does not understand and names the line.
//!
//! It also keeps a YAML dependency out of the graph for a subset this size,
//! which is a licence and advisory surface `deny.toml` would have to carry.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// The delimiter opening and closing a frontmatter block.
const FENCE: &str = "---";

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
/// Each names the line, because a refusal a author cannot locate is one they
/// will work around by deleting the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaError {
    /// The file does not open with a frontmatter fence.
    NoFrontmatter,
    /// The block opened and never closed.
    Unterminated,
    /// A line inside the block is not `key: value`.
    NotAKeyValue {
        line:      usize,
        challenge: String,
    },
    /// A key appeared twice, which silently keeps one of two intents.
    DuplicateKey {
        line: usize,
        key:  String,
    },
    /// A key the generation pass does not know, which is nearly always a typo
    /// for one it does and would otherwise be dropped without a word.
    UnknownKey {
        line: usize,
        key:  String,
    },
    /// A required key is absent.
    Missing {
        key: &'static str,
    },
    /// A key carried a value of the wrong shape.
    BadValue {
        key:    String,
        reason: String,
    },
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFrontmatter => write!(f, "no `---` frontmatter block at the top of the file"),
            Self::Unterminated => write!(f, "the frontmatter block opened and never closed"),
            Self::NotAKeyValue {
                line,
                challenge,
            } => {
                write!(
                    f,
                    "line {line}: not `key: value`, and this reader takes nothing else: {challenge}"
                )
            },
            Self::DuplicateKey {
                line,
                key,
            } => {
                write!(
                    f,
                    "line {line}: `{key}` appears twice, so one of the two intents would be dropped"
                )
            },
            Self::UnknownKey {
                line,
                key,
            } => {
                write!(
                    f,
                    "line {line}: `{key}` is not a field the generation pass reads"
                )
            },
            Self::Missing {
                key,
            } => write!(f, "`{key}` is required and absent"),
            Self::BadValue {
                key,
                reason,
            } => write!(f, "`{key}`: {reason}"),
        }
    }
}

impl std::error::Error for MetaError {}

/// The line separating the card from the elaboration.
///
/// An HTML comment, so the file still reads as markdown anywhere that renders
/// it and the marker does not appear to a human reading the rendered rule.
pub const ELABORATION_MARKER: &str = "<!-- elaboration -->";

/// The frontmatter block and the body after it.
///
/// Returned together because every caller wants both and splitting the file
/// twice is how the two come to disagree about where the body starts.
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

/// Read a rule's meta and body off the text of its full template.
pub fn parse(source: &str) -> Result<Parsed, MetaError> {
    let mut lines = source.lines().enumerate();

    match lines.next() {
        Some((_, first)) if first.trim_end() == FENCE => {},
        _ => return Err(MetaError::NoFrontmatter),
    }

    let mut fields: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut closed_at = None;
    for (idx, line) in lines.by_ref() {
        let n = idx + 1;
        if line.trim_end() == FENCE {
            closed_at = Some(n);
            break;
        }
        // A blank line or a comment inside the block is allowed and carries
        // nothing, which keeps an author from having to pack the fields up.
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            return Err(MetaError::NotAKeyValue {
                line:      n,
                challenge: trimmed.to_string(),
            });
        };
        let key = key.trim().to_string();
        if let Some((first, _)) = fields.get(&key) {
            let _ = first;
            return Err(MetaError::DuplicateKey {
                line: n,
                key,
            });
        }
        if !matches!(key.as_str(), "topics" | "fires" | "kind" | "paths") {
            return Err(MetaError::UnknownKey {
                line: n,
                key,
            });
        }
        fields.insert(key, (n, value.trim().to_string()));
    }

    if closed_at.is_none() {
        return Err(MetaError::Unterminated);
    }

    let topics = match fields.get("topics") {
        Some((_, raw)) => parse_list("topics", raw)?,
        None => {
            return Err(MetaError::Missing {
                key: "topics",
            });
        },
    };
    if topics.is_empty() {
        return Err(MetaError::BadValue {
            key:    "topics".into(),
            reason: "empty, so nothing would ever discover this rule".into(),
        });
    }

    let fires = match fields.get("fires") {
        Some((_, raw)) => unquote(raw),
        None => {
            return Err(MetaError::Missing {
                key: "fires",
            });
        },
    };
    if fires.is_empty() {
        return Err(MetaError::BadValue {
            key:    "fires".into(),
            reason: "empty, and a card without a trigger never connects to the work".into(),
        });
    }

    let kind = match fields.get("kind") {
        Some((_, raw)) => {
            RuleKind::parse(&unquote(raw)).ok_or_else(|| {
                MetaError::BadValue {
                    key:    "kind".into(),
                    reason: format!("`{}` is neither `reflex` nor `discipline`", unquote(raw)),
                }
            })?
        },
        None => {
            return Err(MetaError::Missing {
                key: "kind",
            });
        },
    };

    let paths = match fields.get("paths") {
        Some((_, raw)) => parse_list("paths", raw)?,
        None => Vec::new(),
    };

    // The body starts after the closing fence. Counted from the same walk the
    // fields came from, so it cannot disagree about where that was.
    let body = source
        .lines()
        .skip(closed_at.unwrap_or(0) + 1)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Parsed {
        meta: RuleMeta {
            topics,
            fires,
            kind,
            paths,
        },
        body: body.trim_start_matches('\n').to_string(),
    })
}

/// `[a, b, c]`, and nothing else.
///
/// The block form YAML also allows is refused rather than supported, because
/// supporting one of two spellings quietly is how a corpus ends up with both.
fn parse_list(key: &str, raw: &str) -> Result<Vec<String>, MetaError> {
    let raw = raw.trim();
    let inner = raw
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| {
            MetaError::BadValue {
                key:    key.into(),
                reason: format!("expected an inline list like `[a, b]`, found `{raw}`"),
            }
        })?;
    Ok(inner
        .split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .collect())
}

/// Strip matching surrounding quotes and surrounding space.
fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1 .. s.len() - 1].to_string();
        }
    }
    s.to_string()
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
