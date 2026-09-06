//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a skill declares about itself, and reading it off its template.
//!
//! A skill is a directory, `<name>/`, holding `SKILL.md.tmpl` and whatever it
//! needs beside it: reference documents, scripts, a command-line tool. The body
//! is fetched on demand and is not charged to a session; **the description is,
//! for every skill, on every session**, because the listing carries it so a
//! session can tell which one to reach for.
//!
//! That is why the description is the field with a bound on it and the body is
//! not. A description over the listing budget is dropped rather than refused,
//! so the skill keeps its name and loses the sentence saying when to use it,
//! which is the half that made it findable.
//!
//! The frontmatter syntax and why it is not YAML are in [`crate::frontmatter`].

use serde::Serialize;

use crate::frontmatter::{self, FrontmatterError};

/// Every field a skill may declare.
///
/// `name` and `description` are the corpus's own; the rest are the host's, and
/// are listed so a skill that legitimately uses one is not refused for it. A
/// key outside this set is still refused, since it is nearly always a typo for
/// one inside it and would otherwise be dropped in silence.
const KNOWN: &[&str] = &[
    "name",
    "description",
    "allowed-tools",
    "model",
    "license",
    "disable-model-invocation",
];

/// What a skill declares about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillMeta {
    /// The skill's name, which is how it is invoked.
    pub name:        String,
    /// When to reach for it, which is the whole of what the listing carries.
    pub description: String,
    /// Any host field the skill also declares, kept as written so generation
    /// round-trips it rather than dropping what it does not interpret.
    pub extra:       Vec<(String, String)>,
}

/// Why a skill could not be read.
pub type SkillError = FrontmatterError;

/// The frontmatter block and the body after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub meta: SkillMeta,
    pub body: String,
}

/// Read a skill's meta and body off the text of its `SKILL.md.tmpl`.
pub fn parse(source: &str) -> Result<Parsed, SkillError> {
    let block = frontmatter::split(source, KNOWN)?;

    let name = block.scalar("name").map_err(|e| {
        match e {
            SkillError::BadValue {
                key,
                ..
            } => {
                SkillError::BadValue {
                    key,
                    reason: "empty, and a skill with no name cannot be invoked".into(),
                }
            },
            other => other,
        }
    })?;

    let description = block.scalar("description").map_err(|e| {
        match e {
            SkillError::BadValue {
                key,
                ..
            } => {
                SkillError::BadValue {
                    key,
                    reason: "empty, and the listing carries this instead of the body: without it \
                             the skill is named and nothing says when to reach for it"
                        .into(),
                }
            },
            other => other,
        }
    })?;

    let extra = KNOWN
        .iter()
        .filter(|k| !matches!(**k, "name" | "description"))
        .filter_map(|k| block.optional(k).map(|v| ((*k).to_string(), v)))
        .collect();

    Ok(Parsed {
        meta: SkillMeta {
            name,
            description,
            extra,
        },
        body: block.body,
    })
}

/// Whether a skill's declared name matches the directory holding it.
///
/// The host invokes a skill by its directory, and a session reads the declared
/// name, so a disagreement is a skill that is documented under one name and
/// reachable under another. Nothing else reports it.
pub fn name_matches_dir(meta: &SkillMeta, dir: &str) -> bool {
    meta.name == dir
}
