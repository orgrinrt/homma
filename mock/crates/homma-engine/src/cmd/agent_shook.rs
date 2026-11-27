//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The agent hooks as the manifest homma writes for claude-shook, the Claude
//! Code front of shook, which registers one call per event and runs each
//! event's rows as a chain. homma decides what the rows are; their order,
//! conditions and failures are the front's.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use homma_api::Root;

/// The front's program, found on `PATH`.
pub(crate) const FRONT: &str = "claude-shook";

/// Where the manifest goes, under the workspace root.
pub(crate) const MANIFEST: &str = ".claude/shook/manifests/homma.toml";

/// Seconds a row may run where its registration gave none: the host's own
/// default, so no hook that ran before is cut off by the move.
pub(crate) const DEFAULT_TIMEOUT: u64 = 600;

/// The workspace's notice filter, which member and declared rows run through
/// where the workspace has it.
const FILTER: &str = "tools/hookdedup/hookdedup";

/// What the front reads as the workspace root in an argument.
pub(crate) const ROOT: &str = "{root}";

/// One row of the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub event:    String,
    pub name:     String,
    pub run:      Vec<String>,
    /// Empty for every value.
    pub matcher:  String,
    /// Workspace-relative directories a call must land under; empty for every
    /// call.
    pub paths:    Vec<String>,
    /// Set where the row runs from the workspace root rather than the
    /// session's directory.
    pub at_root:  bool,
    pub timeout:  u64,
    /// Whether it runs through the notice filter where the workspace has one.
    pub filtered: bool,
}

/// `name` as a row name the front takes: anything outside letters, digits,
/// `.`, `_` and `-` becomes `_`.
pub(crate) fn row_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect()
}

/// `rows` with every name unique at its event: a second row under one name
/// takes `-2`, a third `-3`.
pub(crate) fn numbered(rows: Vec<Row>) -> Vec<Row> {
    let mut seen: std::collections::BTreeMap<(String, String), usize> = Default::default();
    rows.into_iter()
        .map(|mut r| {
            let n = seen.entry((r.event.clone(), r.name.clone())).or_insert(0);
            *n += 1;
            if *n > 1 {
                r.name = format!("{}-{n}", r.name);
            }
            r
        })
        .collect()
}

/// The manifest's text: one array per event, in the order the rows came.
pub(crate) fn manifest(rows: &[Row], filter: bool) -> String {
    let mut events: Vec<&str> = Vec::new();
    for r in rows {
        if !events.contains(&r.event.as_str()) {
            events.push(&r.event);
        }
    }
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String("homma".into()));
    for e in events {
        let mut list = Vec::new();
        for r in rows.iter().filter(|r| r.event == e) {
            let mut t = toml::Table::new();
            t.insert("name".into(), r.name.clone().into());
            let mut run: Vec<String> = Vec::new();
            if filter && r.filtered {
                run.push(format!("{ROOT}/{FILTER}"));
                run.push("run".into());
            }
            run.extend(r.run.iter().cloned());
            t.insert(
                "run".into(),
                toml::Value::Array(run.into_iter().map(Into::into).collect()),
            );
            if !r.matcher.is_empty() {
                t.insert("matcher".into(), r.matcher.clone().into());
            }
            if !r.paths.is_empty() {
                t.insert(
                    "paths".into(),
                    toml::Value::Array(r.paths.iter().cloned().map(Into::into).collect()),
                );
            }
            if r.at_root {
                t.insert("dir".into(), ".".into());
            }
            t.insert(
                "timeout".into(),
                toml::Value::Integer(i64::try_from(r.timeout).unwrap_or(i64::MAX)),
            );
            list.push(toml::Value::Table(t));
        }
        table.insert(e.to_string(), toml::Value::Array(list));
    }
    format!(
        "# Written by `homma agent regen`, which rewrites it: the agent hooks of this clone,\n\
         # registered by `claude-shook install`.\n{}",
        toml::to_string(&table).unwrap_or_default()
    )
}

/// The front on `PATH`, or `None`.
pub(crate) fn front() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(FRONT))
        .find(|p| crate::cmd::aggregate::is_executable(p))
}

/// The line a hooks stage refuses with where there is no front.
pub(crate) fn no_front() -> String {
    format!(
        "`{FRONT}` is not on PATH, so the agent hooks are left as they were; install it from \
         git-shook's `claude-shook/`, then run `homma agent regen` again"
    )
}

/// Whether the workspace has the notice filter, executable.
pub(crate) fn has_filter(root: &Path) -> bool {
    crate::cmd::aggregate::is_executable(&root.join(FILTER))
}

/// Write the manifest for `rows` and have `front` register it at the workspace
/// root. A refusal puts the manifest back as it was.
pub(crate) fn install(root: &Root, front: &Path, rows: &[Row]) -> Result<()> {
    let dir = root
        .contain(&root.as_abs().join(".claude/shook/manifests"))
        .map_err(|e| anyhow!("{e}"))?;
    let target = root
        .contain(&root.as_abs().join(MANIFEST))
        .map_err(|e| anyhow!("{e}"))?;
    root.create_dir_all(&dir)?;
    let prior = std::fs::read(target.as_path()).ok();
    root.write(&target, manifest(rows, has_filter(root.as_abs())))?;
    let ran = std::process::Command::new(front)
        .arg("install")
        .arg("--root")
        .arg(root.as_abs().as_path())
        .output();
    let refused = match ran {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => {
            format!(
                "`{FRONT} install` refused, so the agent hooks are left as they were: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )
        },
        Err(e) => {
            format!(
                "{} could not be run, so the agent hooks are left as they were: {e}",
                front.display()
            )
        },
    };
    let _ = match prior {
        Some(bytes) => root.write(&target, bytes),
        None => root.remove_file(&target),
    };
    Err(anyhow!(refused))
}

#[cfg(test)]
#[path = "agent_shook_tests.rs"]
mod tests;
