//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma skills ...`: the workspace's own skill corpus.
//!
//! `list` says what exists and what each one is for. `render` writes the tree a
//! session finds, from the templates that are authored.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use homma_api::SurfacesConfig;
use homma_core::Config;
use homma_org::modes::Modes;
use homma_org::skills::Skills;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Where the corpus is authored, relative to the workspace root.
pub const AUTHORED: &str = ".shared/skills";
/// Where the generated tree goes, relative to the workspace root.
pub const GENERATED: &str = ".claude/skills";

pub fn authored_dir(cfg: &Config) -> PathBuf {
    cfg.workspace.path.join(AUTHORED)
}

pub fn load(dir: &Path) -> Result<Skills> {
    Skills::load(dir).with_context(|| format!("reading the skill corpus at {}", dir.display()))
}

pub mod list {
    use super::*;

    /// Every skill and what it is for.
    #[derive(Debug, Serialize)]
    pub struct ListReport {
        pub skills: Vec<Entry>,
    }

    /// One skill.
    #[derive(Debug, Serialize)]
    pub struct Entry {
        pub name:        String,
        pub description: String,
    }

    impl HumanRender for ListReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            if self.skills.is_empty() {
                return write!(out, "no skills are authored here");
            }
            for (i, s) in self.skills.iter().enumerate() {
                if i > 0 {
                    writeln!(out)?;
                }
                writeln!(out, "  {}", s.name)?;
                write!(out, "    {}", s.description)?;
            }
            Ok(())
        }
    }

    pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
        let corpus = load(&authored_dir(cfg))?;
        emit(
            &ListReport {
                skills: corpus
                    .skills
                    .iter()
                    .map(|s| {
                        Entry {
                            name:        s.name.clone(),
                            description: s.meta.description.clone(),
                        }
                    })
                    .collect(),
            },
            format,
        )?;
        Ok(())
    }
}

/// The `[surfaces]` table, with its defaults where it is absent.
pub fn surfaces(cfg: &Config) -> Result<SurfacesConfig> {
    Ok(match &cfg.surfaces {
        Some(v) => v.clone().try_into().context("reading `[surfaces]`")?,
        None => SurfacesConfig::default(),
    })
}

/// The modes the configuration names, none where their directory is absent.
pub fn modes(cfg: &Config, surfaces: &SurfacesConfig) -> Result<Modes> {
    let dir = cfg.workspace.path.join(surfaces.modes.dir.as_path());
    if !dir.is_dir() {
        return Ok(Modes::default());
    }
    Modes::load(
        &dir,
        &surfaces.modes.skill_prefix,
        surfaces.modes.body_max_chars,
    )
    .with_context(|| format!("reading the modes at {}", dir.display()))
}

pub mod render {
    use super::*;

    /// What the generation pass wrote.
    #[derive(Debug, Serialize)]
    pub struct RenderReport {
        pub authored:  String,
        pub generated: String,
        pub skills:    Vec<String>,
        /// The mode skills written, empty where the gate is off.
        pub modes:     Vec<String>,
        /// Whether the effort keys and the mode skills were kept.
        pub effort:    bool,
        pub files:     usize,
        /// Directories in the generated tree that no authored skill claims.
        pub unclaimed: Vec<String>,
    }

    impl HumanRender for RenderReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            write!(
                out,
                "{} skills, {} files, from {} into {}",
                self.skills.len(),
                self.files,
                self.authored,
                self.generated
            )?;
            if self.effort {
                write!(
                    out,
                    "\n\n{} mode skills, effort keys kept",
                    self.modes.len()
                )?;
            } else {
                write!(
                    out,
                    "\n\neffort keys taken out and no mode skill written: the gate is off for this \
                     session's model"
                )?;
            }
            if !self.unclaimed.is_empty() {
                // Named rather than removed: one is either a skill not moved to
                // the authored side yet or something else entirely, and this
                // pass cannot tell which.
                write!(
                    out,
                    "\n\nin the generated tree and authored nowhere, left alone: {}",
                    self.unclaimed.join(", ")
                )?;
            }
            Ok(())
        }
    }

    /// Render the skills, and the modes where the effort gate keeps them for
    /// `model`.
    pub fn run(cfg: &Config, model: Option<&str>, format: OutputFormat) -> Result<()> {
        let surfaces = surfaces(cfg)?;
        let src = authored_dir(cfg);
        let dst = cfg.workspace.path.join(GENERATED);
        let corpus = load(&src)?;
        let modes = modes(cfg, &surfaces)?;
        if let Some(clash) = corpus.skills.iter().find(|s| modes.claims(&s.name)) {
            anyhow::bail!(
                "the skill `{}` and a mode's skill have the same name, and both would be written to \
                 one directory",
                clash.name
            );
        }
        let effort = surfaces.effort.keeps(model, |v| std::env::var(v).ok());
        let mut written = corpus
            .render(&dst, effort)
            .with_context(|| format!("generating skills into {}", dst.display()))?;
        let mode_names = if effort {
            written.extend(
                modes
                    .render(&dst)
                    .with_context(|| format!("generating mode skills into {}", dst.display()))?,
            );
            modes.modes.iter().map(|m| modes.skill_name(m)).collect()
        } else {
            modes.remove(&dst)?;
            Vec::new()
        };
        let mut unclaimed = corpus.unclaimed(&dst)?;
        unclaimed.retain(|n| !modes.claims(n));
        emit(
            &RenderReport {
                authored: src.display().to_string(),
                generated: dst.display().to_string(),
                skills: corpus.skills.iter().map(|s| s.name.clone()).collect(),
                modes: mode_names,
                effort,
                files: written.len(),
                unclaimed,
            },
            format,
        )?;
        Ok(())
    }
}
