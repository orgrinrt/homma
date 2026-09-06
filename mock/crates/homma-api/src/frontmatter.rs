//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `---` block at the top of an authored file, and the body under it.
//!
//! Shared by every kind of authored document here, because a rule and a skill
//! declare different fields in the same syntax, and two copies of the fence
//! walk would eventually disagree about where the body starts.
//!
//! # Why this is not a YAML parser
//!
//! The syntax is `key: scalar` and `key: [a, b, c]`, and nothing else. A full
//! parser would accept nested maps, anchors, block scalars and multi-document
//! streams, none of which any generation pass here can render or round-trip, so
//! every one of them is a file that parses and then produces something nobody
//! asked for. This refuses what it does not understand and names the line.
//!
//! It also keeps a YAML dependency out of the graph for a subset this size,
//! which is a licence and advisory surface `deny.toml` would have to carry.

use std::collections::BTreeMap;
use std::fmt;

/// The delimiter opening and closing a frontmatter block.
pub const FENCE: &str = "---";

/// Why a frontmatter block could not be read.
///
/// Each names the line, because a refusal an author cannot locate is one they
/// will work around by deleting the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontmatterError {
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
    /// A key the reading pass does not know, which is nearly always a typo for
    /// one it does and would otherwise be dropped without a word.
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

impl fmt::Display for FrontmatterError {
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
            } => write!(f, "line {line}: `{key}` is not a field this pass reads"),
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

impl std::error::Error for FrontmatterError {}

/// A frontmatter block and the body after it.
///
/// Returned together because every caller wants both and splitting the file
/// twice is how the two come to disagree about where the body starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Each declared key, with the line it was declared on and its raw value.
    /// The line is kept so a refusal about a value can name where to look.
    pub fields: BTreeMap<String, (usize, String)>,
    /// Everything after the closing fence, with leading blank lines removed.
    pub body:   String,
}

impl Block {
    /// A required scalar, unquoted and trimmed.
    pub fn scalar(&self, key: &'static str) -> Result<String, FrontmatterError> {
        match self.fields.get(key) {
            Some((_, raw)) => {
                let value = unquote(raw);
                if value.is_empty() {
                    return Err(FrontmatterError::BadValue {
                        key:    key.into(),
                        reason: "empty".into(),
                    });
                }
                Ok(value)
            },
            None => {
                Err(FrontmatterError::Missing {
                    key,
                })
            },
        }
    }

    /// An optional scalar, absent rather than refused when the key is missing.
    pub fn optional(&self, key: &str) -> Option<String> {
        self.fields.get(key).map(|(_, raw)| unquote(raw))
    }

    /// An inline list, `[a, b, c]`, empty when the key is absent.
    ///
    /// For a key that is genuinely optional. A required one takes
    /// [`Self::required_list`], because absent and empty are different mistakes
    /// and telling an author their list is empty when they never wrote the key
    /// sends them to look at the wrong line.
    pub fn list(&self, key: &'static str) -> Result<Vec<String>, FrontmatterError> {
        match self.fields.get(key) {
            Some((_, raw)) => parse_list(key, raw),
            None => Ok(Vec::new()),
        }
    }

    /// An inline list that has to be there, refused by name when it is not.
    pub fn required_list(&self, key: &'static str) -> Result<Vec<String>, FrontmatterError> {
        match self.fields.get(key) {
            Some((_, raw)) => parse_list(key, raw),
            None => {
                Err(FrontmatterError::Missing {
                    key,
                })
            },
        }
    }
}

/// Read the frontmatter block off a file, taking only the keys in `known`.
///
/// An unknown key is refused rather than dropped: it is nearly always a typo
/// for one that is known, and dropping it produces a file that parses and then
/// behaves as though the author had written nothing.
pub fn split(source: &str, known: &[&str]) -> Result<Block, FrontmatterError> {
    let mut lines = source.lines().enumerate();

    match lines.next() {
        Some((_, first)) if first.trim_end() == FENCE => {},
        _ => return Err(FrontmatterError::NoFrontmatter),
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
            return Err(FrontmatterError::NotAKeyValue {
                line:      n,
                challenge: trimmed.to_string(),
            });
        };
        let key = key.trim().to_string();
        if fields.contains_key(&key) {
            return Err(FrontmatterError::DuplicateKey {
                line: n,
                key,
            });
        }
        if !known.contains(&key.as_str()) {
            return Err(FrontmatterError::UnknownKey {
                line: n,
                key,
            });
        }
        fields.insert(key, (n, value.trim().to_string()));
    }

    let Some(closed_at) = closed_at else {
        return Err(FrontmatterError::Unterminated);
    };

    // The body starts after the closing fence. Counted from the same walk the
    // fields came from, so it cannot disagree about where that was.
    let body = source
        .lines()
        .skip(closed_at + 1)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Block {
        fields,
        body: body.trim_start_matches('\n').to_string(),
    })
}

/// `[a, b, c]`, and nothing else.
///
/// The block form YAML also allows is refused rather than supported, because
/// supporting one of two spellings quietly is how a corpus ends up with both.
pub fn parse_list(key: &str, raw: &str) -> Result<Vec<String>, FrontmatterError> {
    let raw = raw.trim();
    let inner = raw
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| {
            FrontmatterError::BadValue {
                key:    key.into(),
                reason: format!("expected an inline list like `[a, b]`, found `{raw}`"),
            }
        })?;
    Ok(split_outside_quotes(inner)
        .iter()
        .map(|s| unquote(s))
        .filter(|s| !s.is_empty())
        .collect())
}

/// Split on the commas that separate elements, not on the ones inside them.
///
/// `paths` is the field that carries globs and a brace group is the ordinary
/// way to write one, so `["src/**/*.{ts,tsx}"]` is a list of one element with a
/// comma in it. Splitting on every comma turns that into `"src/**/*.{ts` and
/// `tsx}"`, two patterns with unbalanced quotes that match nothing, and the
/// generated card then declares gating on nothing rather than failing to parse.
/// A rule whose gating the host cannot read is promoted to always loaded, so
/// the only sign was the always-loaded count rising by two.
///
/// What is guarded is a quoted element containing commas. There is no escape
/// handling, because the format has no escape: a quote of the same kind inside
/// a quoted element is not expressible either way, and an unterminated quote
/// takes the rest of the list as one element rather than erroring, which
/// `unquote` then leaves with its opening quote attached where a reader sees it.
fn split_outside_quotes(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in inner.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                cur.push(c);
            },
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                cur.push(c);
            },
            None if c == ',' => out.push(core::mem::take(&mut cur)),
            None => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Strip matching surrounding quotes and surrounding space.
pub fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1 .. s.len() - 1].to_string();
        }
    }
    s.to_string()
}
