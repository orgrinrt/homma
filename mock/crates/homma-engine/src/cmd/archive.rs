//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma archive <repo> [--from <forge>] [--owner <owner>]`.
//!
//! Marks the named repo as archived on the named forge (read-only flag).
//! Does not delete; the repo stays visible as a frozen artefact. This is
//! the deliberate second phase of a migration: run only after the
//! destination repo is verified end-to-end.
//!
//! Defaults come from `homma.toml`: `--from` falls back to
//! `[repos.<repo>].forge`, `--owner` to `[repos.<repo>].owner`.

use std::io::Write;

use anyhow::{Context, Result, anyhow};
use homma_core::Config;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::forge::{client_from_config, resolve_forge};
use crate::output::{HumanRender, emit};

/// Result payload for `homma archive`.
#[derive(Debug, Serialize)]
pub struct ArchiveReport {
    pub forge: String,
    pub owner: String,
    pub name:  String,
}

impl HumanRender for ArchiveReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        writeln!(
            out,
            "archived {}/{} on {}",
            self.owner, self.name, self.forge
        )
    }
}

pub fn run(
    cfg: &Config,
    repo_name: &str,
    from: Option<&str>,
    owner: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let repo_cfg = cfg
        .repo(repo_name)
        .ok_or_else(|| anyhow!("no repository named `{repo_name}` under the workspace root"))?;
    // The flag wins, then what the clone's own remote says. Neither is an
    // error rather than a guess: a repository with a local remote has no forge
    // and no owner, and archiving is an outward act that has to land where it
    // was meant to.
    let forge_name = match from.or(repo_cfg.forge.as_deref()) {
        Some(f) => f,
        None => {
            return Err(anyhow!(
                "`{repo_name}` has no forge: its origin remote names none this \
                 workspace has a profile for. Pass --from."
            ));
        },
    };
    let forge_cfg = resolve_forge(cfg, forge_name)?;
    let owner = match owner.or(repo_cfg.owner.as_deref()) {
        Some(o) => o,
        None => {
            return Err(anyhow!(
                "`{repo_name}` has no owner: its origin remote names none. Pass \
                 --owner."
            ));
        },
    };

    let client = client_from_config(forge_cfg);
    client
        .archive_repo(owner, repo_name)
        .with_context(|| format!("archiving {owner}/{repo_name} on {forge_name}"))?;

    emit(
        &ArchiveReport {
            forge: forge_name.into(),
            owner: owner.into(),
            name:  repo_name.into(),
        },
        format,
    )?;
    Ok(())
}
