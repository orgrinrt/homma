//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma identity lease ...`: who holds an identity now.
//!
//! The rule is homma-org's `decide` and the write is homma-core's swap; this
//! reads the record off the trunk, asks the rule, writes the answer, and on a
//! trunk that moved under it reads and asks again.

use std::path::Path;

use anyhow::{Context, Result, bail};
use homma_api::Workspace;
use homma_core::release::git;
use homma_core::swap::{Swapped, swap_file};
use homma_org::catalogue::is_agent;
use homma_org::lease::{Ask, Lease, Outcome, decide};
use jiff::Timestamp;

/// How many times a swap is tried before the trunk is taken to be too busy.
pub const TRIES: usize = 5;

/// Where a handle's record sits under the workspace root.
pub fn record_path(ws: &Workspace, handle: &str) -> String {
    format!("{}/{handle}.toml", ws.paths.leases)
}

/// The record on the remote trunk now, none when the identity is free, and
/// the tip it was read at.
pub fn read(root: &Path, ws: &Workspace, handle: &str) -> Result<(Option<Lease>, String)> {
    let tip = git::fetch_branch(root, &ws.lease.remote, &ws.lease.branch)
        .with_context(|| format!("fetching `{}` from `{}`", ws.lease.branch, ws.lease.remote))?;
    let path = record_path(ws, handle);
    let text = git::show(root, &tip, &path)?;
    let lease = text
        .map(|t| Lease::parse(&t).with_context(|| format!("reading the lease `{path}` at {tip}")))
        .transpose()?;
    Ok((lease, tip))
}

/// What a run left on the trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// This record is the lease now.
    Held(Lease),
    /// The identity is free.
    Free,
}

/// Ask `ask` of `handle` at `now`, and write the answer onto the trunk.
pub fn run(
    root: &Path,
    ws: &Workspace,
    handle: &str,
    ask: Ask<'_>,
    minutes: Option<u32>,
    now: Timestamp,
) -> Result<Done> {
    let Some(id) = ws.org.get(handle) else {
        bail!("`{handle}` is no handle of the catalogue; a lease is taken by handle");
    };
    if !is_agent(id.role) {
        bail!("`{handle}` is the king, who is a person and holds no lease");
    }
    let minutes = minutes.unwrap_or(ws.lease.minutes);
    let path = record_path(ws, handle);
    for _ in 0 .. TRIES {
        let (current, tip) = read(root, ws, handle)?;
        let outcome = decide(current.as_ref(), handle, ask, now, minutes)
            .with_context(|| format!("`{handle}`"))?;
        let (content, done, message) = match outcome {
            Outcome::Nothing => return Ok(Done::Free),
            Outcome::Remove => (None, Done::Free, format!("lease: release {handle}")),
            Outcome::Write(lease) => {
                let message = match &lease.replaces {
                    Some(old) if current.as_ref().is_some_and(|c| c.session == *old) => {
                        format!("lease: {handle} taken over from {old} for {}", lease.job)
                    },
                    _ => format!("lease: {handle} held for {}", lease.job),
                };
                (Some(lease.render()), Done::Held(lease), message)
            },
        };
        let message = format!("{message}\n\nsession {}", session_of(ask));
        match swap_file(
            root,
            &ws.lease.remote,
            &ws.lease.branch,
            &tip,
            &path,
            content.as_deref(),
            &message,
        )? {
            Swapped::Landed(_) => return Ok(done),
            Swapped::Moved => continue,
        }
    }
    bail!(
        "`{}` on `{}` moved under each of {TRIES} tries; nothing was written",
        ws.lease.branch,
        ws.lease.remote
    )
}

fn session_of<'a>(ask: Ask<'a>) -> &'a str {
    match ask {
        Ask::Take {
            session,
            ..
        }
        | Ask::Renew {
            session,
        }
        | Ask::Release {
            session,
        } => session,
    }
}

/// One line saying who holds `handle`, or that nobody does.
pub fn describe(handle: &str, lease: Option<&Lease>) -> String {
    match lease {
        None => format!("{handle} free"),
        Some(l) => {
            let replaced = l
                .replaces
                .as_deref()
                .map(|r| format!(", replacing {r}"))
                .unwrap_or_default();
            format!(
                "{handle} held by {} for {} since {} until {}{replaced}",
                l.session, l.job, l.taken, l.expires
            )
        },
    }
}

#[cfg(test)]
mod tests;
