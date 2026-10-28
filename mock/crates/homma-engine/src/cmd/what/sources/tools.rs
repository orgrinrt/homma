//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tools under `tools/`, read the way `homma tools` reads them.

use std::path::Path;

use super::super::doc::{Doc, Gap, Gathered, Kind, Verb};
use crate::cmd::tools::descriptor::{Entry, TOOLS_DIR, discover};

/// Usage lines kept from one readme.
const MAX_USAGE_LINES: usize = 80;
/// Characters of prose kept from one readme.
const MAX_PROSE: usize = 24_000;

/// The usage lines and the prose of a tool's readme.
///
/// A usage line is a line of a fenced block that begins with the tool's name or one of its commands' names and
/// carries words after it. Its summary is the text after a gap of two spaces, and its command is the line without
/// that text, the tool's own name replaced by `head`. The prose is everything outside the fences.
fn readme(text: &str, tool: &str, commands: &[&str], head: &str) -> (Vec<Verb>, String) {
    let mut verbs: Vec<Verb> = Vec::new();
    let mut prose = String::new();
    let mut fenced = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            if prose.len() < MAX_PROSE {
                prose.push_str(t);
                prose.push(' ');
            }
            continue;
        }
        let first = t.split_whitespace().next().unwrap_or("");
        if first != tool && !commands.contains(&first) {
            continue;
        }
        let (usage, summary) = t
            .find("  ")
            .map_or((t, ""), |i| (t[.. i].trim_end(), t[i ..].trim()));
        let rest = usage[first.len() ..].trim();
        if rest.is_empty() || verbs.len() >= MAX_USAGE_LINES {
            continue;
        }
        let command =
            if first == tool { format!("{head} {rest}") } else { format!("{head} {usage}") };
        if verbs.iter().any(|v| v.command == command) {
            continue;
        }
        verbs.push(Verb {
            name: rest.to_string(),
            summary: summary.to_string(),
            command,
            example: true,
        });
    }
    (verbs, prose.trim_end().to_string())
}

/// The readme beside a tool's descriptor, or nothing where it is absent, unreadable or a link.
fn readme_of(root: &Path, name: &str) -> Option<String> {
    let p = root.join(TOOLS_DIR).join(name).join("README.md");
    if !std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file()) {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

pub fn gather(root: &Path) -> Gathered {
    if !root.join("tools").is_dir() {
        return Gathered {
            docs: Vec::new(),
            gaps: vec![Gap {
                source: "tools",
                count:  0,
                what:   "no tools/ here: tools not searched".to_string(),
            }],
        };
    }
    let mut docs = Vec::new();
    let mut broken = 0;
    for e in discover(root) {
        match e {
            Entry::Tool(t) => {
                let head = if t.promote {
                    format!("homma {}", t.name)
                } else {
                    format!("homma tools run {}", t.name)
                };
                // A tool whose verbs have no default one is refused without a verb, so the bare command names them.
                let bare = if t.commands.is_empty() || t.commands.iter().any(|c| c.name == t.name) {
                    head.clone()
                } else {
                    let names: Vec<&str> = t.commands.iter().map(|c| c.name.as_str()).collect();
                    format!("{head} <{}>", names.join("|"))
                };
                let mut d = Doc::new(Kind::Tool, &t.name, &bare, &t.summary);
                d.tags.clone_from(&t.tags);
                d.verbs = t
                    .commands
                    .iter()
                    .map(|c| {
                        Verb {
                            name:    c.name.clone(),
                            summary: c.summary.clone(),
                            command: if c.name == t.name {
                                head.clone()
                            } else {
                                format!("{head} {}", c.name)
                            },
                            example: false,
                        }
                    })
                    .collect();
                if let Some(text) = readme_of(root, &t.name) {
                    let names: Vec<&str> = t.commands.iter().map(|c| c.name.as_str()).collect();
                    let (verbs, prose) = readme(&text, &t.name, &names, &head);
                    for v in verbs {
                        if !d.verbs.iter().any(|seen| seen.command == v.command) {
                            d.verbs.push(v);
                        }
                    }
                    d.body = prose;
                }
                docs.push(d);
            },
            Entry::Broken(_) => broken += 1,
        }
    }
    let mut gaps = Vec::new();
    if broken > 0 {
        gaps.push(Gap {
            source: "tools",
            count:  broken,
            what:   format!("{broken} tool.toml cannot be read (`homma tools check` names them)"),
        });
    }
    Gathered {
        docs,
        gaps,
    }
}
