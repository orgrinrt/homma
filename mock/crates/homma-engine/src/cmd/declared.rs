//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The hooks a workspace declares for itself, under `[agent.hooks]` in the
//! manifest.
//!
//! Each row becomes an entry of the dispatcher's, from [`FROM_ROWS`]. A row
//! naming repositories is narrowed to calls landing in them, the way a member's
//! hook is narrowed to its own; one naming none sees every call.

use homma_api::{AgentTable, Root};

use crate::cmd::aggregate;
use crate::cmd::dispatch::{Entry, FROM_ROWS};

/// What the rows produced: an entry for each row that could be carried, and
/// each that could not, with why.
#[derive(Debug, Default)]
pub(crate) struct Declared {
    pub entries:  Vec<Entry>,
    pub problems: Vec<String>,
}

/// Make an entry of every row of `agent`'s table.
///
/// `repos` is the manifest's `(name, workspace-relative path)` pairs, the same
/// list the workspace gate is written from. A row naming no `timeout` gets
/// `[agent] timeout`.
///
/// A row narrowed to a repository outside `scope` is skipped quietly; one narrowed to a name
/// that is neither in `repos` nor outside the scope is a problem.
pub(crate) fn declared_entries(
    root: &Root,
    agent: &AgentTable,
    repos: &[(String, String)],
    scope: &homma_core::scope::Scope,
) -> Declared {
    let mut out = Declared::default();

    'rows: for (event, row) in agent.hooks.iter() {
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

        out.entries.push(Entry {
            event: event.to_string(),
            matcher: row.matcher().to_string(),
            run: crate::cmd::util::relative_str(std::path::Path::new(row.run())),
            runner: String::new(),
            args: String::new(),
            roots,
            timeout: row.timeout().unwrap_or(agent.timeout()),
            background: row.background(),
            from: FROM_ROWS.to_string(),
        });
    }
    out
}

#[cfg(test)]
#[path = "declared_tests.rs"]
mod tests;
