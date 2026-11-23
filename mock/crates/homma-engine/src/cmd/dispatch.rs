//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The one dispatcher every agent hook runs through, and the table it reads.
//!
//! The regen gathers an [`Entry`] for every hook a member gives, every row the
//! manifest declares and the workspace gate, writes them to
//! `.claude/hooks/_dispatch.json`, writes the dispatcher beside it, and returns
//! its registrations for [`super::aggregate::merge_settings`]. A tool event
//! whose answers fold ([`FOLDED`]) gets one registration for the hooks the call
//! waits on and one for the `async` ones, and the dispatcher runs them side by
//! side and folds what they say. Every other event gets one registration per
//! entry, so the host reads each answer itself. The dispatcher is a shell
//! script, so a tool call never waits on homma's engine being built for a new
//! pin.

use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result, anyhow};
use homma_api::Root;
use serde::{Deserialize, Serialize};

use crate::cmd::aggregate::{HookEntry, MANAGED_MARK, carries_the_mark};

#[path = "dispatch_script.rs"]
mod script;
pub(crate) use script::dispatcher_script;

/// The dispatcher's file name under `.claude/hooks/`.
pub(crate) const DISPATCHER: &str = "_dispatch.sh";

/// The table's file name under `.claude/hooks/`.
pub(crate) const TABLE: &str = "_dispatch.json";

/// The `from` of an entry made from a row of the manifest.
pub(crate) const FROM_ROWS: &str = "_declared";

/// The `from` of the workspace gate's entry.
pub(crate) const FROM_GATE: &str = "_workspace";

/// The events whose hooks the dispatcher runs together and folds: the tool
/// events, where hooks are many, every answer is JSON the host folds the same
/// way, and an exit 2 is a deny before the call and a block after it.
pub(crate) const FOLDED: &[&str] = &["PreToolUse", "PostToolUse"];

/// The seconds the host gives the dispatcher over its longest entry, so the
/// host never cuts short a hook the dispatcher would have let run.
const SLACK: u32 = 10;

/// One hook the dispatcher runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// The host event it runs at.
    pub event:      String,
    /// What it fires for, as the host reads a matcher; empty or `*` for
    /// everything.
    pub matcher:    String,
    /// The file it runs, relative to the workspace, or absolute for a member
    /// declared outside it.
    pub run:        String,
    /// The program it is run with, as written; empty where the file runs itself.
    pub runner:     String,
    /// Its arguments, as written: words, never a line of shell.
    pub args:       String,
    /// The repositories it is narrowed to, relative to the workspace; empty for
    /// every call.
    pub roots:      Vec<String>,
    /// The seconds it may run.
    pub timeout:    u32,
    /// Whether it runs with the hooks nothing waits on.
    #[serde(rename = "async")]
    pub background: bool,
    /// The member it came from, [`FROM_ROWS`] or [`FROM_GATE`].
    pub from:       String,
}

/// The table as it is written: the line that makes it homma's, and the entries.
#[derive(Debug, Serialize, Deserialize)]
struct Table {
    written: String,
    entries: Vec<Entry>,
}

/// The entries the last regen wrote, or none where there is no table, or one
/// homma did not write, or one it cannot read.
pub(crate) fn previous(root: &Root) -> Vec<Entry> {
    let path = root.as_abs().join(".claude/hooks").join(TABLE);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Table>(&s).ok())
        .filter(|t| t.written.contains(MANAGED_MARK))
        .map(|t| t.entries)
        .unwrap_or_default()
}

/// Whether the table at `path` is absent or homma's.
fn table_is_ours(path: &std::path::Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Table>(&s).ok())
        .is_some_and(|t| t.written.contains(MANAGED_MARK))
}

/// Whether a registration runs the dispatcher.
pub(crate) fn is_dispatcher_command(cmd: &str) -> bool {
    crate::cmd::aggregate::hook_file_named(cmd).is_some_and(|n| n == DISPATCHER)
}

/// Write the table and the dispatcher, and return the dispatcher's
/// registrations.
///
/// `filter` is `[agent] filter`, the program the dispatcher runs itself
/// through. A table or a dispatcher homma did not write is refused by path
/// rather than written over. The wrappers an earlier regen wrote are left for
/// [`retire_wrappers`], once the settings that name them are rewritten.
pub(crate) fn install(root: &Root, entries: &[Entry], filter: &[String]) -> Result<Vec<HookEntry>> {
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow!("{e}"))?;
    root.create_dir_all(&hooks_dir).ok();
    let script = root
        .contain_under(&hooks_dir, DISPATCHER)
        .map_err(|e| anyhow!("{e}"))?;
    let table = root
        .contain_under(&hooks_dir, TABLE)
        .map_err(|e| anyhow!("{e}"))?;
    if script.as_path().exists() && !carries_the_mark(script.as_path()) {
        return Err(anyhow!(
            "`.claude/hooks/{DISPATCHER}` was not written by homma, so the dispatcher is not \
             written over it; move that file aside or rename it"
        ));
    }
    if !table_is_ours(table.as_path()) {
        return Err(anyhow!(
            "`.claude/hooks/{TABLE}` was not written by homma, so the dispatcher's table is not \
             written over it; move that file aside or rename it"
        ));
    }

    let body = serde_json::to_string_pretty(&Table {
        written: format!("this file is written {MANAGED_MARK}"),
        entries: entries.to_vec(),
    })?;
    root.write(&table, body + "\n")
        .with_context(|| format!("write {}", table.as_path().display()))?;
    root.write(&script, dispatcher_script(filter))
        .with_context(|| format!("write {}", script.as_path().display()))?;
    #[cfg(unix)]
    root.set_executable(&script)?;
    Ok(registrations(entries))
}

/// Remove every file under `.claude/hooks/` an earlier regen wrote as a wrapper:
/// whatever carries the mark but the dispatcher and the workspace gate. Run once
/// the settings no longer name them, so a merge that failed leaves them running.
pub(crate) fn retire_wrappers(root: &Root) -> Result<()> {
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow!("{e}"))?;
    if !hooks_dir.as_path().is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(hooks_dir.as_path())
        .with_context(|| format!("read {}", hooks_dir.as_path().display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let Some(n) = name.to_str() else {
            continue;
        };
        if n == DISPATCHER || n == crate::cmd::gates::GATE_SCRIPT_NAME {
            continue;
        }
        let target = root
            .contain_under(&hooks_dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if target.as_path().is_file() && carries_the_mark(target.as_path()) {
            root.remove_file(&target).ok();
        }
    }
    Ok(())
}

/// The dispatcher's registrations. A folded event gets one per mode that has
/// entries: their alternatives joined, or every tool where one entry takes every
/// tool, and the longest timeout with [`SLACK`] over. Any other event gets one
/// per entry, under the entry's own matcher, timeout and `async`, naming the
/// entry by its place in the table.
pub(crate) fn registrations(entries: &[Entry]) -> Vec<HookEntry> {
    let mut folded: BTreeMap<(&str, bool), Vec<&Entry>> = BTreeMap::new();
    let mut alone: BTreeMap<&str, Vec<(usize, &Entry)>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if FOLDED.contains(&e.event.as_str()) {
            folded
                .entry((e.event.as_str(), e.background))
                .or_default()
                .push(e);
        } else {
            alone.entry(e.event.as_str()).or_default().push((i, e));
        }
    }
    let command =
        |tail: String| format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{DISPATCHER} {tail}");
    let mut out: Vec<HookEntry> = folded
        .into_iter()
        .map(|((event, background), es)| {
            let longest = es.iter().map(|e| e.timeout).max().unwrap_or(0);
            let mode = if background { "async" } else { "sync" };
            HookEntry {
                event: event.to_string(),
                matcher: joined(&es),
                command: command(format!("{event} {mode}")),
                timeout: Some(longest.saturating_add(SLACK)),
                background,
            }
        })
        .collect();
    for (event, es) in alone {
        for (i, e) in es {
            out.push(HookEntry {
                event:      event.to_string(),
                matcher:    if e.matcher.trim().is_empty() {
                    String::new()
                } else {
                    e.matcher.clone()
                },
                command:    command(format!("{event} one {i}")),
                timeout:    Some(e.timeout),
                background: e.background,
            });
        }
    }
    out
}

/// The matcher of a folded registration: none where no entry names one, every
/// tool where any takes every tool, and otherwise the entries' alternatives,
/// each once, in the order first met.
fn joined(es: &[&Entry]) -> String {
    let none = |e: &&Entry| e.matcher.trim().is_empty();
    if es.iter().all(none) {
        return String::new();
    }
    if es.iter().any(|e| none(e) || e.matcher.trim() == "*") {
        return "*".to_string();
    }
    let mut seen: Vec<String> = Vec::new();
    for e in es {
        for piece in alternatives(&e.matcher) {
            if !seen.contains(&piece) {
                seen.push(piece);
            }
        }
    }
    seen.join("|")
}

/// The alternatives a matcher takes, each whole: the names of a list of exact
/// names, split on `|` and `,`, or the branches of a regular expression, split on
/// a `|` outside any group, class or escape, an empty branch kept since it takes
/// every name. Joined with `|` they take what the matchers took between them.
fn alternatives(matcher: &str) -> Vec<String> {
    let plain = matcher
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ' | ',' | '|'));
    if plain {
        return matcher
            .split(['|', ','])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    let mut out = Vec::new();
    let (mut depth, mut class, mut escaped) = (0usize, false, false);
    let mut start = 0;
    for (i, c) in matcher.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '[' if !class => class = true,
            ']' if class => class = false,
            '(' if !class => depth += 1,
            ')' if !class => depth = depth.saturating_sub(1),
            '|' if !class && depth == 0 => {
                out.push(matcher[start .. i].to_string());
                start = i + 1;
            },
            _ => {},
        }
    }
    out.push(matcher[start ..].to_string());
    out
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
pub(in crate::cmd) mod tests;

#[cfg(test)]
#[path = "dispatch_run_tests.rs"]
mod run_tests;

#[cfg(test)]
#[path = "dispatch_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "dispatch_fold_tests.rs"]
mod fold_tests;
