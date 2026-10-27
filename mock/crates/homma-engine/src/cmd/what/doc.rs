//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a source hands over: a thing that does something, in the words it describes itself with.

use clap::ValueEnum;
use serde::Serialize;

/// The kind of a thing, in the order that breaks a tie between equal scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A directory under `tools/` with a `tool.toml`.
    Tool,
    /// An executable under `.shared/scripts`.
    Script,
    /// A subcommand of homma.
    Verb,
    /// A line of `cargo mock tools`.
    Mock,
    /// A skill under `.shared/skills`.
    Skill,
    /// A rule under `.shared/rules`.
    Rule,
}

impl Kind {
    /// The word printed beside a hit.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Script => "script",
            Self::Verb => "verb",
            Self::Mock => "mock",
            Self::Skill => "skill",
            Self::Rule => "rule",
        }
    }
}

impl Kind {
    /// What a match in this kind is worth against the others: the tools are what is asked for, and a
    /// rule or a skill is advice beside them.
    pub const fn prior(self) -> f64 {
        match self {
            Self::Tool | Self::Script | Self::Verb | Self::Mock => 1.0,
            Self::Skill => 0.8,
            Self::Rule => 0.6,
        }
    }
}

/// One verb of a thing: a tool's command, a script's usage line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verb {
    pub name:    String,
    pub summary: String,
    /// The exact command that runs it.
    pub command: String,
}

/// One thing that can be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    pub kind:    Kind,
    pub name:    String,
    /// The exact command that runs it, or reaches it.
    pub command: String,
    pub summary: String,
    /// The rest of what it says about itself, where a source has more than the summary.
    pub body:    String,
    pub tags:    Vec<String>,
    pub verbs:   Vec<Verb>,
    /// Where it sits, as words: the directories of a script, the repository of a mockspace command.
    pub place:   Vec<String>,
}

impl Doc {
    /// A thing with a name, a command and a summary, and nothing else yet.
    pub fn new(kind: Kind, name: &str, command: &str, summary: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
            command: command.to_string(),
            summary: summary.to_string(),
            body: String::new(),
            tags: Vec::new(),
            verbs: Vec::new(),
            place: Vec::new(),
        }
    }
}

/// Something a source could not give, said in the answer's footer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Gap {
    pub source: &'static str,
    pub count:  usize,
    pub what:   String,
}

/// What one source read.
#[derive(Debug, Default)]
pub struct Gathered {
    pub docs: Vec<Doc>,
    pub gaps: Vec<Gap>,
}

/// Which kinds a query asks about.
pub fn wanted(kinds: &[Kind], k: Kind) -> bool {
    kinds.is_empty() || kinds.contains(&k)
}
