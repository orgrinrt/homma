//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The hooks a workspace declares for itself, under `[agent.hooks]` in the
//! manifest.
//!
//! Each row becomes a wrapper at `<workspace>/.claude/hooks/_declared--*.sh`,
//! carrying the same mark as every aggregated one, so the rows are kept and
//! removed by the same rule. A row naming repositories is narrowed to calls
//! landing in them, the way an aggregated hook is narrowed to its own; one
//! naming none sees every call.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use anyhow::{Context, Result, anyhow};
use homma_api::{AgentHooks, Root};

use crate::cmd::aggregate::{self, HookEntry, LANDS_IN_SH, MANAGED_MARK, sh_single_quote_escape};

/// The prefix every declared wrapper's file name starts with, which no
/// repository name can produce, since the aggregated ones start with a name
/// and `--`.
pub(crate) const DECLARED_PREFIX: &str = "_declared--";

/// Whether a registration names a declared wrapper.
pub(crate) fn is_declared_command(cmd: &str) -> bool {
    aggregate::hook_file_named(cmd).is_some_and(|n| n.starts_with(DECLARED_PREFIX))
}

/// What the rows produced: a registration for each wrapper written, and each
/// row that could not be carried, with why.
#[derive(Debug, Default)]
pub(crate) struct Declared {
    pub entries:  Vec<HookEntry>,
    pub problems: Vec<String>,
}

/// Write a wrapper for every row, remove the declared wrappers no row wants
/// any more, and return the registrations for [`aggregate::merge_settings`].
///
/// `repos` is the manifest's `(name, workspace-relative path)` pairs, the same
/// list the workspace gate is written from.
pub(crate) fn install_declared(
    root: &Root,
    hooks: &AgentHooks,
    repos: &[(String, String)],
) -> Result<Declared> {
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow!("{e}"))?;
    root.create_dir_all(&hooks_dir).ok();

    let mut out = Declared::default();
    let mut written: BTreeSet<String> = BTreeSet::new();
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();

    'rows: for (event, row) in hooks.iter() {
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
        for r in row.repos() {
            match repos.iter().find(|(n, _)| n == r) {
                Some((_, p)) => roots.push(crate::cmd::util::relative_str(std::path::Path::new(p))),
                None => {
                    out.problems.push(format!(
                        "{shown} is narrowed to `{r}`, which the workspace has no repository \
                         by; not registered"
                    ));
                    continue 'rows;
                },
            }
        }

        let base = format!("{DECLARED_PREFIX}{event}--{}", slug(row.run()));
        let n = taken.entry(base.clone()).or_insert(0);
        *n += 1;
        let name = if *n == 1 { format!("{base}.sh") } else { format!("{base}--{n}.sh") };
        let target = root
            .contain_under(&hooks_dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if target.as_path().exists() && !aggregate::carries_the_mark(target.as_path()) {
            out.problems.push(format!(
                "`.claude/hooks/{name}` was not written by homma, so {shown} is not written \
                 over it; move that file aside or rename it"
            ));
            continue;
        }
        root.write(&target, declared_wrapper(event, row.run(), &roots))
            .with_context(|| format!("write {}", target.as_path().display()))?;
        #[cfg(unix)]
        root.set_executable(&target)?;
        out.entries.push(HookEntry {
            event:   event.to_string(),
            matcher: row.matcher().to_string(),
            command: format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{name}"),
        });
        written.insert(name);
    }

    // A declared wrapper no row wrote this time is one whose row is gone, and
    // is removed if it is homma's. One that is not keeps its place.
    for entry in fs::read_dir(hooks_dir.as_path())
        .with_context(|| format!("read {}", hooks_dir.as_path().display()))?
    {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !name.starts_with(DECLARED_PREFIX) || written.contains(&name) {
            continue;
        }
        let target = root
            .contain_under(&hooks_dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if aggregate::carries_the_mark(target.as_path()) {
            root.remove_file(&target).ok();
        }
    }
    Ok(out)
}

/// A path as the part of a file name that says which script a wrapper runs:
/// every character outside letters, digits, `-` and `_` becomes `_`.
fn slug(run: &str) -> String {
    run.chars()
        .map(
            |c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }
            },
        )
        .collect()
}

/// The wrapper for one row. Like an aggregated wrapper it finds the workspace
/// from its own location and names no machine; unlike one, it runs a script of
/// the workspace's own, and with no repositories to narrow to it hands every
/// call straight on.
fn declared_wrapper(event: &str, run: &str, roots: &[String]) -> String {
    let run_q = sh_single_quote_escape(run);
    let mut out = format!(
        r##"#!/usr/bin/env bash
# Declared under `[agent.hooks.{event}]` in the manifest, written {MANAGED_MARK}.
# Runs {run}, relative to the workspace this file sits in.

set -u

WS=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")/../.." && pwd) || exit 0
HOOK="$WS"/'{run_q}'

# Not in this tree: there is nothing here to run, and declining is the honest
# answer rather than an approval nobody made.
[ -x "$HOOK" ] || exit 0
"##
    );
    if roots.is_empty() {
        out.push_str("\nexec \"$HOOK\" \"$@\"\n");
        return out;
    }
    let list: Vec<String> = roots
        .iter()
        .map(|r| format!("'{}'", sh_single_quote_escape(r)))
        .collect();
    let list = list.join(" ");
    out.push_str(
        r##"
INPUT=$(cat)

# Without `jq` this cannot read where the call lands, so it forwards rather
# than guessing, which runs the hook more often than it needs and never skips it.
if ! command -v jq >/dev/null 2>&1; then
    exec "$HOOK" "$@" <<<"$INPUT"
fi

"##,
    );
    out.push_str(LANDS_IN_SH);
    out.push_str(&format!(
        r##"
for rel in {list}; do
    case "$rel" in
        /*) root="$rel" ;;
        *)  root="$WS/$rel" ;;
    esac
    if lands_in "$root"; then
        exec "$HOOK" "$@" <<<"$INPUT"
    fi
done
exit 0
"##
    ));
    out
}

#[cfg(test)]
#[path = "declared_tests.rs"]
mod tests;
