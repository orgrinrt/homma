//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The workspace's own commands a release runs, each named under `[paths]` of
//! the manifest: `forge_route`, which the tags and releases go through, and
//! `history_scan`, which reads the history a release would publish before
//! anything moves. homma names neither; the workspace does, and keeps what
//! each one knows.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow};
use homma_api::Paths;
use homma_api::path::RelPath;

/// The command `pick` takes from `[paths]` of the manifest at `manifest`,
/// absolute from the manifest's directory. A manifest that names none leaves
/// the release without it. One that does not read as a workspace's is an
/// error, since taking it as naming none would quietly skip what the command
/// is there for, and so is a command it names that is not there.
fn workspace_command(
    manifest: &Path,
    key: &str,
    pick: impl FnOnce(Paths) -> Option<RelPath>,
) -> Result<Option<PathBuf>> {
    let ws = crate::cmd::org::load(manifest)?;
    let Some(rel) = pick(ws.paths) else {
        return Ok(None);
    };
    let at = manifest
        .parent()
        .unwrap_or(Path::new("."))
        .join(rel.as_path());
    let at = std::path::absolute(&at).unwrap_or(at);
    if !at.is_file() {
        return Err(anyhow!(
            "`{key}` in {} names {}, which is not there",
            manifest.display(),
            at.display()
        ));
    }
    Ok(Some(at))
}

/// The command the workspace's tags and releases go through. Taking an
/// unreadable manifest as naming none would send the tag down the git push the
/// route is there to avoid, after `main` has moved, and a route named and not
/// there would fail at step 6.
pub(super) fn forge_route_of(manifest: &Path) -> Result<Option<PathBuf>> {
    workspace_command(manifest, "forge_route", |p| p.forge_route)
}

/// The command that reads a repository's whole history for attribution the
/// workspace forbids. Taking an unreadable manifest as naming none would
/// release a history nobody read.
pub(super) fn history_scan_of(manifest: &Path) -> Result<Option<PathBuf>> {
    workspace_command(manifest, "history_scan", |p| p.history_scan)
}

/// Why the history of the clone at `root` may not be released, or `None`
/// where `scan` reads it clean. Exit 0 is clean; any other exit, and a scan
/// that could not run at all, refuses, since a scan that did not happen reads
/// like a clean one.
pub(super) fn history_refusal(scan: &Path, root: &Path) -> Option<String> {
    let out = match Command::new(scan).arg(root).current_dir(root).output() {
        Ok(out) => out,
        Err(e) => {
            return Some(format!(
                "the history scan {} could not run ({e}), so the history is unread and nothing is \
                 released",
                scan.display()
            ));
        },
    };
    if out.status.success() {
        return None;
    }
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let verdict = match out.status.code() {
        Some(1) => "carries attribution the workspace forbids".to_string(),
        Some(n) => format!("could not be scanned (the scan exited {n})"),
        None => "could not be scanned (the scan was killed)".to_string(),
    };
    Some(format!(
        "the history {verdict}, so nothing is released from it. A release publishes the history \
         whole; the remedy is to rehouse the repository onto a clean history, \
         which is a human's decision. What {} said:\n{}",
        scan.display(),
        said.trim_end()
    ))
}
