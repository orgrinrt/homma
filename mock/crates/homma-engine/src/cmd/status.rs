//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma status`: report the workspace state captured by `homma.toml`.
//!
//! No network calls, no git ops. Just reads the config and summarises what
//! the workspace is. Git working-tree status for individual repos lives
//! under `homma repo status <name>` for now.

use std::io::Write;

use anyhow::Result;
use homma_core::{Config, ForgeKind};
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Top-level success payload for `homma status`.
#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub workspace: WorkspaceLine,
    pub forges:    Vec<ForgeLine>,
    pub repos:     Vec<RepoLine>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceLine {
    pub name:                   String,
    pub path:                   String,
    pub default_public_branch:  String,
    pub default_working_branch: String,
}

#[derive(Debug, Serialize)]
pub struct ForgeLine {
    pub name:      String,
    pub kind:      String,
    pub base_url:  String,
    pub api_url:   String,
    pub token_env: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RepoLine {
    pub name:           String,
    /// Absent where the clone's remote names no configured forge, which is a
    /// real state rather than a gap: a member with a local remote has none.
    pub forge:          Option<String>,
    /// Absent on the same terms.
    pub owner:          Option<String>,
    pub local_path:     String,
    pub public_branch:  String,
    pub working_branch: String,
}

pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
    let report = build_report(cfg);
    emit(&report, format)?;
    Ok(())
}

fn build_report(cfg: &Config) -> StatusReport {
    let workspace = WorkspaceLine {
        name:                   cfg.workspace.name.clone(),
        path:                   cfg.workspace.path.display().to_string(),
        default_public_branch:  cfg.defaults.public_branch.clone(),
        default_working_branch: cfg.defaults.working_branch.clone(),
    };
    let forges = cfg
        .forges
        .iter()
        .map(|(name, f)| {
            ForgeLine {
                name:      name.clone(),
                kind:      forge_kind_str(f.kind).to_string(),
                base_url:  f.base_url.clone(),
                api_url:   f.api_url.clone(),
                token_env: f.token_env.clone(),
            }
        })
        .collect();
    let repos = cfg
        .repos
        .iter()
        .map(|(name, r)| {
            RepoLine {
                name:           name.clone(),
                forge:          r.forge.clone(),
                owner:          r.owner.clone(),
                local_path:     r.local_path.display().to_string(),
                public_branch:  r.resolved_public_branch(&cfg.defaults).to_string(),
                working_branch: r.resolved_working_branch(&cfg.defaults).to_string(),
            }
        })
        .collect();
    StatusReport {
        workspace,
        forges,
        repos,
    }
}

fn forge_kind_str(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::Github => "github",
        ForgeKind::Forgejo => "forgejo",
    }
}

impl HumanRender for StatusReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        writeln!(
            out,
            "workspace: {} ({})",
            self.workspace.name, self.workspace.path
        )?;
        writeln!(
            out,
            "  default public={}  working={}",
            self.workspace.default_public_branch, self.workspace.default_working_branch
        )?;
        if !self.forges.is_empty() {
            writeln!(out)?;
            writeln!(out, "forges:")?;
            for f in &self.forges {
                let tok = f.token_env.as_deref().unwrap_or("<none>");
                writeln!(
                    out,
                    "  {} [{}] {} (api={}, token_env={})",
                    f.name, f.kind, f.base_url, f.api_url, tok
                )?;
            }
        }
        if !self.repos.is_empty() {
            writeln!(out)?;
            writeln!(out, "repos:")?;
            for r in &self.repos {
                // A member with no forge prints as one rather than as a blank
                // where a name should be, so the line says which state it is
                // in instead of leaving the reader to guess at an empty field.
                let on = match (&r.owner, &r.forge) {
                    (Some(owner), Some(forge)) => format!("{owner}/{} on {forge}", r.name),
                    (Some(owner), None) => format!("{owner}/{}, forge unknown", r.name),
                    (None, _) => format!("{}, no forge remote", r.name),
                };
                writeln!(
                    out,
                    "  {} -> {on} (path={}, public={}, working={})",
                    r.name, r.local_path, r.public_branch, r.working_branch
                )?;
            }
        }
        Ok(())
    }
}
