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
use homma_api::{Identity, Workspace};
use homma_core::release::{git, sh};
use homma_core::swap::{Swapped, swap_file};
use homma_org::catalogue::{is_agent, resolve};
use homma_org::lease::{Ask, Lease, Outcome, decide};
use jiff::Timestamp;

/// How many times a swap is tried before the trunk is taken to be too busy.
pub const TRIES: usize = 5;

/// Where a handle's record sits under the workspace root.
pub fn record_path(ws: &Workspace, handle: &str) -> String {
    format!("{}/{handle}.toml", ws.paths.leases)
}

/// The shortest and the longest a lease may last, in minutes: one minute, and
/// a week, the span a journeyman is reserved for.
pub const MINUTES: std::ops::RangeInclusive<u32> = 1 ..= 7 * 24 * 60;

/// The agent a handle or an alias names, refusing a name the catalogue does
/// not hold and the king, who is a person and holds no lease.
pub fn entry<'a>(ws: &'a Workspace, name: &str) -> Result<&'a Identity> {
    let Some(id) = resolve(ws, name) else {
        bail!("`{name}` is no handle or alias of the catalogue");
    };
    if !is_agent(id.role) {
        bail!("`{name}` is the king, who is a person and holds no lease");
    }
    Ok(id)
}

/// The record on the remote trunk now, none when the identity is free, and
/// the tip it was read at.
///
/// A record that is there and cannot be read, or that names another handle
/// than its file, is refused: taking it for a free identity would let a take
/// overwrite a holder nobody could read.
pub fn read(root: &Path, ws: &Workspace, handle: &str) -> Result<(Option<Lease>, String)> {
    let tip = git::fetch_branch(root, &ws.lease.remote, &ws.lease.branch)
        .with_context(|| format!("fetching `{}` from `{}`", ws.lease.branch, ws.lease.remote))?;
    let path = record_path(ws, handle);
    let listed = sh::run(root, "git", &["ls-tree", "--name-only", &tip, "--", &path])?;
    anyhow::ensure!(
        listed.ok(),
        "listing `{path}` at {tip}: {}",
        listed.stderr.trim()
    );
    if listed.stdout.trim().is_empty() {
        return Ok((None, tip));
    }
    let Some(text) = git::show(root, &tip, &path)? else {
        bail!("the lease `{path}` is at {tip} and cannot be read");
    };
    let lease =
        Lease::parse(&text).with_context(|| format!("reading the lease `{path}` at {tip}"))?;
    anyhow::ensure!(
        lease.handle == handle,
        "the lease `{path}` at {tip} names `{}`, not `{handle}`",
        lease.handle
    );
    Ok((Some(lease), tip))
}

/// What a run left on the trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// This record is the lease now.
    Held(Lease),
    /// The identity is free.
    Free,
}

/// Ask `ask` of the identity `name` names at `now`, and write the answer onto
/// the trunk.
pub fn run(
    root: &Path,
    ws: &Workspace,
    name: &str,
    ask: Ask<'_>,
    minutes: Option<u32>,
    now: Timestamp,
) -> Result<Done> {
    let handle = entry(ws, name)?.handle.as_str();
    let minutes = minutes.unwrap_or(ws.lease.minutes);
    anyhow::ensure!(
        MINUTES.contains(&minutes),
        "a lease of {minutes} minutes; one lasts from {} minute to a week ({} minutes)",
        MINUTES.start(),
        MINUTES.end()
    );
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
