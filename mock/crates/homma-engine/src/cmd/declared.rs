//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The hooks a workspace declares for itself, under `[agent.hooks]` in the
//! manifest.
//!
//! Each row becomes a row of the manifest homma writes for claude-shook,
//! named `_declared--<slug>`. A row naming repositories is narrowed to calls
//! landing in them by `paths`, the way an aggregated hook is narrowed to its
//! own; one naming none sees every call.

use homma_api::{AgentTable, Root};

use crate::cmd::agent_shook::{self, ROOT, Row};
use crate::cmd::aggregate;

/// The prefix every declared wrapper an older homma wrote started with, which
/// no repository name can produce, since the aggregated ones start with a name
/// and `--`. Kept to recognise those wrappers' registrations.
pub(crate) const DECLARED_PREFIX: &str = "_declared--";

/// Whether a registration names a declared wrapper, a row's or the nudges'.
pub(crate) fn is_declared_command(cmd: &str) -> bool {
    aggregate::hook_file_named(cmd).is_some_and(|n| is_declared_name(&n))
}

/// Whether a file name under `.claude/hooks/` is a declared wrapper's.
fn is_declared_name(name: &str) -> bool {
    name.starts_with(DECLARED_PREFIX) || name.starts_with(crate::cmd::nudge::NUDGES_PREFIX)
}

/// What the rows produced: a row of the manifest for each, and each row that
/// could not be carried, with why.
#[derive(Debug, Default)]
pub(crate) struct Declared {
    pub rows:     Vec<Row>,
    pub problems: Vec<String>,
}

/// A row of the manifest for every declared row, then the nudges' rows.
///
/// `repos` is the manifest's `(name, workspace-relative path)` pairs, the same
/// list the workspace gate is written from.
///
/// A row narrowed to a repository outside `scope` is skipped quietly; one narrowed to a name
/// that is neither in `repos` nor outside the scope is a problem.
pub(crate) fn declared_rows(
    root: &Root,
    table: &AgentTable,
    repos: &[(String, String)],
    scope: &homma_core::scope::Scope,
) -> Declared {
    let mut out = Declared::default();
    'rows: for (event, row) in table.hooks.iter() {
        let shown = format!("`[[agent.hooks.{event}]]` running `{}`", row.run());
        let script = root.as_abs().join(row.run());
        if !script.is_file() {
            out.problems.push(format!(
                "{shown} names no file under the workspace; not registered"
            ));
            continue;
        }
        if !aggregate::is_executable(&script) {
            out.problems.push(format!(
                "{shown} names a file that is not executable, so the host could not run it; \
                 not registered"
            ));
            continue;
        }
        let mut roots = Vec::new();
        let mut outside = 0usize;
        for r in row.repos() {
            match repos.iter().find(|(n, _)| n == r) {
                Some((_, p)) => roots.push(crate::cmd::util::relative_str(std::path::Path::new(p))),
                None => {
                    // a repository outside this clone's scope is left out without a word: it
                    // exists, the work is just not this clone's. The row keeps the names that
                    // are in scope.
                    if !scope.admits(r) {
                        outside += 1;
                        continue;
                    }
                    out.problems.push(format!(
                        "{shown} is narrowed to `{r}`, which the workspace has no repository \
                         by; not registered"
                    ));
                    continue 'rows;
                },
            }
        }
        // a row with no names reads as applying to every call, so one whose every name is
        // outside the scope is dropped rather than widened
        if outside > 0 && roots.is_empty() {
            continue;
        }
        let run = crate::cmd::util::relative_str(std::path::Path::new(row.run()));
        out.rows.push(Row {
            event:    event.to_string(),
            name:     agent_shook::row_name(&format!("{DECLARED_PREFIX}{run}")),
            run:      vec![format!("{ROOT}/{run}")],
            matcher:  row.matcher().to_string(),
            paths:    roots,
            at_root:  false,
            timeout:  agent_shook::DEFAULT_TIMEOUT,
            filtered: true,
        });
    }
    out.rows.extend(crate::cmd::nudge::rows(&table.nudges));
    out
}

#[cfg(test)]
#[path = "declared_tests.rs"]
mod tests;
