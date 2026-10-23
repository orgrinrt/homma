//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading the descriptors of the workspace's own tools.
//!
//! A tool is a directory `tools/<name>/` holding a `tool.toml` in the format renki defines for a host
//! to read. This reads the same fields and refuses what renki refuses, so a descriptor that parses here
//! is one the host half of renki accepts. Nothing is fetched and nothing runs: reading a descriptor is
//! the whole of listing a tool.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where the tools sit, relative to the workspace root.
pub const TOOLS_DIR: &str = "tools";
/// The descriptor's file name.
pub const DESCRIPTOR: &str = "tool.toml";

/// One verb of a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verb {
    /// What it is invoked as.
    pub name:    String,
    /// One line.
    pub summary: String,
    /// The executable, relative to the tool's directory.
    pub run:     String,
}

/// A tool whose descriptor was read and accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tool {
    /// The tool's name, which is its directory's.
    pub name:    String,
    /// One line.
    pub summary: String,
    /// For finding a tool without knowing its name.
    pub tags:    Vec<String>,
    /// Which backend materialises it. Only `local` runs here.
    pub backend: String,
    /// Whether it is also a subcommand of homma.
    pub promote: bool,
    /// The tool's directory, absolute.
    pub dir:     PathBuf,
    /// Every verb it offers, in the order the descriptor lists them.
    pub verbs:   Vec<Verb>,
}

/// A directory with a descriptor that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Broken {
    /// The directory's name, which is the name the tool would have.
    pub name: String,
    /// The descriptor.
    pub file: PathBuf,
    /// Why it was refused.
    pub why:  String,
}

/// What `discover` found in one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A usable tool.
    Tool(Tool),
    /// A descriptor that is not.
    Broken(Broken),
}

impl Entry {
    /// The name, which is the directory's.
    pub fn name(&self) -> &str {
        match self {
            Self::Tool(t) => &t.name,
            Self::Broken(b) => &b.name,
        }
    }
}

#[derive(Deserialize)]
struct Outer {
    tool: Raw,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name:     String,
    summary:  String,
    #[serde(default)]
    tags:     Vec<String>,
    backend:  String,
    #[allow(dead_code)]
    source:   Source,
    #[serde(default)]
    commands: Vec<RawVerb>,
    #[serde(default)]
    promote:  bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Source {
    Git {
        #[allow(dead_code)]
        url: String,
        #[allow(dead_code)]
        rev: String,
    },
    Path {
        #[allow(dead_code)]
        path: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVerb {
    name:    String,
    summary: String,
    run:     String,
}

/// Whether `s` is a name a verb or a tool may have.
fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Read one descriptor. `dir_name` is the name of the directory it sat in and `dir` the directory.
///
/// # Errors
///
/// With the reason, when the text is not a descriptor renki accepts, or names a tool other than the
/// directory's, or a verb twice, or a `run` that leaves the tool.
pub fn parse(dir_name: &str, text: &str, dir: &Path) -> Result<Tool, String> {
    let raw = toml::from_str::<Outer>(text)
        .map_err(|e| format!("could not read it as a tool descriptor: {}", e.message()))?
        .tool;
    if raw.name != dir_name {
        return Err(format!(
            "its name is `{}` and the directory's is `{dir_name}`; they are the same name",
            raw.name
        ));
    }
    if !plain(&raw.name) {
        return Err(format!(
            "`{}` is not a name: lowercase letters, digits, `.`, `_` and `-`",
            raw.name
        ));
    }
    for (what, value) in [("summary", &raw.summary), ("backend", &raw.backend)] {
        if value.trim().is_empty() {
            return Err(format!("its {what} is empty"));
        }
    }
    let mut verbs: Vec<Verb> = Vec::new();
    for v in raw.commands {
        if !plain(&v.name) {
            return Err(format!(
                "the command `{}` is not a name: lowercase letters, digits, `.`, `_` and `-`",
                v.name
            ));
        }
        if verbs.iter().any(|seen| seen.name == v.name) {
            return Err(format!("the command `{}` is given twice", v.name));
        }
        let run = Path::new(&v.run);
        let climbs = run
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)));
        if v.run.trim().is_empty() || run.is_absolute() || climbs {
            return Err(format!(
                "the command `{}` runs `{}`, which is not a path inside the tool",
                v.name, v.run
            ));
        }
        verbs.push(Verb {
            name:    v.name,
            summary: v.summary,
            run:     v.run,
        });
    }
    Ok(Tool {
        name: raw.name,
        summary: raw.summary,
        tags: raw.tags,
        backend: raw.backend,
        promote: raw.promote,
        dir: dir.to_path_buf(),
        verbs,
    })
}

/// Every directory under `<root>/tools` that holds a `tool.toml`, as a tool or as the reason it is not,
/// sorted by name. A directory without a descriptor is not a tool and is not listed. No tools directory
/// is no tools.
pub fn discover(root: &Path) -> Vec<Entry> {
    let base = root.join(TOOLS_DIR);
    let Ok(read) = std::fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = Vec::new();
    for e in read.flatten() {
        let dir = e.path();
        let file = dir.join(DESCRIPTOR);
        if !dir.is_dir() || !file.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let broken = |why: String| {
            Entry::Broken(Broken {
                name: name.clone(),
                file: file.clone(),
                why,
            })
        };
        out.push(match std::fs::read_to_string(&file) {
            Err(e) => broken(format!("could not be read: {e}")),
            Ok(text) => {
                match parse(&name, &text, &dir) {
                    Ok(t) => Entry::Tool(t),
                    Err(why) => broken(why),
                }
            },
        });
    }
    out.sort_by(|a, b| a.name().cmp(b.name()));
    out
}
