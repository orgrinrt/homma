//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The wrapper the regen writes for each event the nudges name, beside the
//! declared ones and swept by the same rule.

use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use homma_api::agent_hooks::EVERY_TOOL;
use homma_api::{ContainedPath, Nudges, Root};

use crate::cmd::aggregate::{self, HookEntry, MANAGED_MARK, sh_single_quote_escape};
use crate::cmd::declared::Declared;

/// The prefix of a nudge wrapper's file name, which no repository name can
/// produce, as the declared prefix cannot.
pub(crate) const NUDGES_PREFIX: &str = "_nudges--";

/// Write one wrapper per event the rows name into `hooks_dir`, add its
/// registration and its problems to `out`, and its name to `written`, so the
/// sweep that follows keeps it.
pub(crate) fn install_wrappers(
    root: &Root,
    hooks_dir: &ContainedPath,
    nudges: &Nudges,
    out: &mut Declared,
    written: &mut BTreeSet<String>,
) -> Result<()> {
    for event in nudges.events() {
        let name = format!("{NUDGES_PREFIX}{event}.sh");
        let target = root
            .contain_under(hooks_dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if target.as_path().exists() && !aggregate::carries_the_mark(target.as_path()) {
            out.problems.push(format!(
                "`.claude/hooks/{name}` was not written by homma, so the nudges on `{event}` are \
                 not written over it; move that file aside or rename it"
            ));
            continue;
        }
        root.write(&target, wrapper(event))
            .with_context(|| format!("write {}", target.as_path().display()))?;
        #[cfg(unix)]
        root.set_executable(&target)?;
        out.entries.push(HookEntry {
            event:   event.to_string(),
            matcher: matcher(nudges, event),
            command: format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{name}"),
        });
        written.insert(name);
    }
    Ok(())
}

/// The rows' tools on `event` joined, every tool where any row names none.
pub(crate) fn matcher(nudges: &Nudges, event: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for row in nudges.of_event(event) {
        let m = row.matcher();
        if m == EVERY_TOOL {
            return EVERY_TOOL.to_string();
        }
        if !seen.contains(&m) {
            seen.push(m);
        }
    }
    seen.join("|")
}

/// The wrapper for one event. It finds the workspace from where it sits, and
/// where there is no `homma` to run there is nothing it could decide, so it
/// lets the call through.
fn wrapper(event: &str) -> String {
    let event_q = sh_single_quote_escape(event);
    format!(
        r##"#!/usr/bin/env bash
# The nudges under `[agent.nudges]` on `{event}`, written {MANAGED_MARK}.

set -u

WS=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")/../.." && pwd) || exit 0
command -v homma >/dev/null 2>&1 || exit 0
cd -- "$WS" || exit 0
exec homma agent nudge '{event_q}'
"##
    )
}
