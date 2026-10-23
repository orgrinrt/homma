//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tools as the engine reaches them where renki builds: the listing, the check, and the run of a tool.
//!
//! Everything here reads or runs a tool through `renki::extension`, so the module exists on unix only and
//! [`super::refused`] answers the same four functions elsewhere.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};
use clap::CommandFactory;
use serde::Serialize;

use super::descriptor::{self, Entry, discover};
use super::dispatch::{exec, plan};
use super::{Asked, cache, groups};
use crate::cli::{Cli, OutputFormat};
use crate::cmd::Outcome;
use crate::output::{HumanRender, emit};

/// Run the tool `name` with `words`, replacing homma. Returns only to refuse.
///
/// # Errors
///
/// Whatever [`plan`] refuses, or a process that could not be started.
pub fn run_tool(root: &Path, name: &str, words: &[OsString], asked: Asked) -> Result<Outcome> {
    let entries = discover(root);
    let c = plan(&entries, name, words, asked, root, &cache::root)?;
    // Everything homma has printed is out before the tool's own output follows it.
    let _ = std::io::stdout().flush();
    match exec(c)? {}
}

/// `homma <group> <word> ...` for a word the built-in group does not know: the tool's, when there is one.
///
/// # Errors
///
/// The refusal names the built-in verbs beside the tool's, since the word was neither.
pub fn run_merged(root: &Path, group: &str, words: &[OsString]) -> Result<Outcome> {
    let entries = discover(root);
    let known = Cli::command()
        .find_subcommand(group)
        .map(|sc| {
            sc.get_subcommands()
                .map(|s| s.get_name().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match plan(
        &entries,
        group,
        words,
        Asked::AsSubcommand,
        root,
        &cache::root,
    ) {
        Ok(c) => {
            let _ = std::io::stdout().flush();
            match exec(c)? {}
        },
        Err(e) => {
            bail!(
                "{e}; the built-in verbs of `{group}` are {}",
                known.join(", ")
            )
        },
    }
}

/// One tool, as listed.
#[derive(Debug, Serialize)]
pub struct ToolLine {
    pub name:    String,
    pub summary: String,
    pub verbs:   Vec<String>,
    pub promote: bool,
    /// How it relates to a built-in of the same name, if there is one.
    pub builtin: Option<&'static str>,
}

/// What `homma tools` answers.
#[derive(Debug, Serialize)]
pub struct ToolsReport {
    pub tools:  Vec<ToolLine>,
    pub broken: Vec<descriptor::Broken>,
}

fn builtin_relation(name: &str) -> Option<&'static str> {
    Cli::command().find_subcommand(name)?;
    Some(if groups::groups().iter().any(|g| g == name) { "merged" } else { "shadowed" })
}

/// The listing for the tools under `root`.
pub fn report(root: &Path) -> ToolsReport {
    let mut tools = Vec::new();
    let mut broken = Vec::new();
    for e in discover(root) {
        match e {
            Entry::Tool(t) => {
                tools.push(ToolLine {
                    builtin: builtin_relation(&t.name),
                    verbs:   t.commands.iter().map(|c| c.name.clone()).collect(),
                    name:    t.name,
                    summary: t.summary,
                    promote: t.promote,
                })
            },
            Entry::Broken(b) => broken.push(b),
        }
    }
    ToolsReport {
        tools,
        broken,
    }
}

impl HumanRender for ToolsReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.tools.is_empty() && self.broken.is_empty() {
            return write!(
                out,
                "no tools: no tools/<name>/tool.toml under the workspace"
            );
        }
        let w = self.tools.iter().map(|t| t.name.len()).max().unwrap_or(0);
        let mut first = true;
        for t in &self.tools {
            if !first {
                writeln!(out)?;
            }
            first = false;
            write!(
                out,
                "{:<w$}  {}  [{}]",
                t.name,
                t.summary,
                t.verbs.join(", ")
            )?;
            match (t.promote, t.builtin) {
                (_, Some("shadowed")) => {
                    write!(
                        out,
                        "  (a built-in holds the name: `homma tools run {} ...`)",
                        t.name
                    )?
                },
                (_, Some(_)) => write!(out, "  (merged with the built-in of the same name)")?,
                (false, None) => write!(out, "  (not promoted: `homma tools run {} ...`)", t.name)?,
                (true, None) => {},
            }
        }
        for b in &self.broken {
            if !first {
                writeln!(out)?;
            }
            first = false;
            write!(
                out,
                "{:<w$}  BROKEN {}: {}",
                b.name,
                b.file.display(),
                b.why
            )?;
        }
        Ok(())
    }
}

/// `homma tools`.
///
/// # Errors
///
/// When the listing cannot be written.
pub fn list(root: &Path, format: OutputFormat) -> Result<Outcome> {
    emit(&report(root), format)?;
    Ok(Outcome::Ok)
}

/// `homma tools check`: the listing's own exit status. Non-zero while any descriptor is broken, with
/// the broken ones and nothing else printed.
///
/// # Errors
///
/// When the report cannot be written.
pub fn check(root: &Path, format: OutputFormat) -> Result<Outcome> {
    let mut r = report(root);
    let bad = !r.broken.is_empty();
    r.tools.clear();
    if bad {
        emit(&r, format)?;
        return Ok(Outcome::ReportedFailure);
    }
    Ok(Outcome::Ok)
}
