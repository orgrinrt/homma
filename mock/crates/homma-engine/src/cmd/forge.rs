//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma forge ...` commands.
//!
//! These exercise the [`homma_core::Forge`] trait. `show` and `exists`
//! cover the read-side surface; mutating ops (create, archive, delete) are
//! exposed via [`super::migrate`] and [`super::archive`].

use std::io::Write;

use anyhow::{Context, Result, anyhow};
use homma_core::{
    Config,
    Forge,
    ForgeConfig,
    ForgeKind,
    ForgejoClient,
    GitHubClient,
    RepoMetadata,
    Visibility,
};
use serde::Serialize;

use crate::cli::{ForgeOp, OutputFormat};
use crate::output::HumanRender;

/// Construct a boxed [`Forge`] client from a config entry.
///
/// Boxed because `ForgejoClient` and `GitHubClient` are distinct types with
/// distinct internal state; the command surface treats them uniformly via
/// the trait.
pub(crate) fn client_from_config(forge: &ForgeConfig) -> Box<dyn Forge> {
    match forge.kind {
        ForgeKind::Forgejo => Box::new(ForgejoClient::new(forge)),
        ForgeKind::Github => Box::new(GitHubClient::new(forge)),
    }
}

/// Resolve a forge profile from the config by name, returning an error if
/// the profile is not declared.
pub(crate) fn resolve_forge<'a>(cfg: &'a Config, name: &str) -> Result<&'a ForgeConfig> {
    cfg.forge(name)
        .ok_or_else(|| anyhow!("forge `{name}` not declared in [forges.*]"))
}

pub mod show {
    use super::*;

    #[derive(Debug, Serialize)]
    pub struct ShowReport {
        pub forge:           String,
        pub owner:           String,
        pub name:            String,
        pub description:     Option<String>,
        pub default_branch:  String,
        pub visibility:      String,
        pub topics:          Vec<String>,
        pub archived:        bool,
        pub clone_url_https: String,
    }

    impl HumanRender for ShowReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            writeln!(out, "{}/{} on {}", self.owner, self.name, self.forge)?;
            writeln!(
                out,
                "  default_branch={}  visibility={}  archived={}",
                self.default_branch, self.visibility, self.archived
            )?;
            if let Some(desc) = &self.description {
                writeln!(out, "  description: {desc}")?;
            }
            if !self.topics.is_empty() {
                writeln!(out, "  topics: {}", self.topics.join(", "))?;
            }
            writeln!(out, "  clone_url: {}", self.clone_url_https)?;
            Ok(())
        }
    }

    pub fn run(cfg: &Config, forge_name: &str, slug: &str, format: OutputFormat) -> Result<()> {
        let (owner, name) = ForgeOp::parse_slug(slug)?;
        let forge_cfg = resolve_forge(cfg, forge_name)?;
        let client = client_from_config(forge_cfg);
        let meta = client
            .fetch_repo(owner, name)
            .with_context(|| format!("fetching {}/{} from {}", owner, name, forge_name))?;
        let report = to_report(forge_name, meta);
        crate::output::emit(&report, format)?;
        Ok(())
    }

    fn to_report(forge_name: &str, meta: RepoMetadata) -> ShowReport {
        ShowReport {
            forge:           forge_name.into(),
            owner:           meta.owner,
            name:            meta.name,
            description:     meta.description,
            default_branch:  meta.default_branch,
            visibility:      visibility_str(meta.visibility).into(),
            topics:          meta.topics,
            archived:        meta.archived,
            clone_url_https: meta.clone_url_https,
        }
    }

    fn visibility_str(v: Visibility) -> &'static str {
        match v {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Internal => "internal",
        }
    }
}

pub mod exists {
    use super::*;

    #[derive(Debug, Serialize)]
    pub struct ExistsReport {
        pub forge:  String,
        pub owner:  String,
        pub name:   String,
        pub exists: bool,
    }

    impl HumanRender for ExistsReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            writeln!(
                out,
                "{}/{} on {}: {}",
                self.owner,
                self.name,
                self.forge,
                if self.exists { "exists" } else { "absent" }
            )
        }
    }

    pub fn run(cfg: &Config, forge_name: &str, slug: &str, format: OutputFormat) -> Result<()> {
        let (owner, name) = ForgeOp::parse_slug(slug)?;
        let forge_cfg = resolve_forge(cfg, forge_name)?;
        let client = client_from_config(forge_cfg);
        let exists = client
            .repo_exists(owner, name)
            .with_context(|| format!("checking {}/{} on {}", owner, name, forge_name))?;
        let report = ExistsReport {
            forge: forge_name.into(),
            owner: owner.into(),
            name: name.into(),
            exists,
        };
        crate::output::emit(&report, format)?;
        Ok(())
    }
}
