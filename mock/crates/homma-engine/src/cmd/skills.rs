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
use homma_core::Config;
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

pub mod render {
    use super::*;

    /// What the generation pass wrote.
    #[derive(Debug, Serialize)]
    pub struct RenderReport {
        pub authored:  String,
        pub generated: String,
        pub skills:    Vec<String>,
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

    pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
        let src = authored_dir(cfg);
        let dst = cfg.workspace.path.join(GENERATED);
        let corpus = load(&src)?;
        let written = corpus
            .render(&dst)
            .with_context(|| format!("generating skills into {}", dst.display()))?;
        let unclaimed = corpus.unclaimed(&dst)?;
        emit(
            &RenderReport {
                authored: src.display().to_string(),
                generated: dst.display().to_string(),
                skills: corpus.skills.iter().map(|s| s.name.clone()).collect(),
                files: written.len(),
                unclaimed,
            },
            format,
        )?;
        Ok(())
    }
}
