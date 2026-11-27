//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The row the regen writes for each event the nudges name, into the manifest
//! it writes for claude-shook.

use homma_api::Nudges;
use homma_api::agent_hooks::EVERY_TOOL;

use crate::cmd::agent_shook::{self, Row};

/// The prefix of the nudge wrappers an older homma wrote, kept to recognise
/// their registrations; no repository name can produce it, as the declared
/// prefix cannot.
pub(crate) const NUDGES_PREFIX: &str = "_nudges--";

/// One row per event the nudge rows name, running `homma agent nudge <Event>`
/// from the workspace root with the payload on stdin. It does not run through
/// the notice filter: a nudge's `ttl` is that rule for its own text.
pub(crate) fn rows(nudges: &Nudges) -> Vec<Row> {
    nudges
        .events()
        .into_iter()
        .map(|event| {
            Row {
                event:    event.to_string(),
                name:     "_nudges".to_string(),
                run:      vec!["homma".into(), "agent".into(), "nudge".into(), event.to_string()],
                matcher:  matcher(nudges, event),
                paths:    Vec::new(),
                at_root:  true,
                timeout:  agent_shook::DEFAULT_TIMEOUT,
                filtered: false,
            }
        })
        .collect()
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
