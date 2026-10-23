//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma tools`, and `homma <tool> <verb>` for the tools that are promoted.
//!
//! The content repository carries tools of its own under `tools/<name>/`, each described by a
//! `tool.toml`. They are found by reading, listed by `homma tools`, and run by replacing homma with the
//! command the descriptor names. The built-in commands win over a tool of the same name, except that the
//! groups in [`groups::groups`] hand the verbs they do not know to the tool.
//!
//! Reading and running a tool is renki's, and renki builds on unix only. The parts that reach it sit
//! behind `cfg(unix)` ([`host`]); elsewhere [`refused`] answers the same commands with [`unix_only`]. What
//! is portable is the root of the workspace, how a tool was asked for, and the group's word handed on.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cmd::Outcome;

#[cfg(unix)]
pub mod cache;
#[cfg(unix)]
pub mod descriptor;
#[cfg(unix)]
pub mod dispatch;
pub mod groups;
#[cfg(unix)]
mod host;
#[cfg(not(unix))]
mod refused;

#[cfg(unix)]
pub use host::{check, list, run_merged, run_tool};
#[cfg(not(unix))]
pub use refused::{check, list, run_merged, run_tool};

/// How the tool was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// As a subcommand of homma, which only a promoted tool answers.
    AsSubcommand,
    /// By `homma tools run`, which any tool in the workspace answers.
    ByName,
}

/// Why a command that reaches a tool is refused where renki does not build.
#[cfg_attr(unix, allow(dead_code))]
pub fn unix_only() -> anyhow::Error {
    anyhow::anyhow!(
        "the workspace's tools are not available on this platform: they run on renki, which builds on unix only"
    )
}

/// The workspace root: where the manifest is, or the working directory when there is no `--dir`.
pub fn root(config: &Path) -> PathBuf {
    let dir = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// `homma <group> <word> ...` as [`groups::parse`] handed it over.
///
/// # Errors
///
/// As [`run_merged`].
pub fn run_handed(h: groups::Handed) -> Result<Outcome> {
    let manifest = crate::cmd::manifest_path(h.config.as_deref(), h.dir.as_deref());
    run_merged(&root(&manifest), &h.group, &h.words)
}

#[cfg(all(test, unix))]
mod cache_tests;
#[cfg(all(test, unix))]
mod descriptor_tests;
#[cfg(all(test, unix))]
mod dispatch_tests;
#[cfg(test)]
mod groups_tests;
#[cfg(all(test, unix))]
mod tests;
