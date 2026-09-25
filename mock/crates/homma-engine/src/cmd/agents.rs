//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma agents ...`: the workspace's own personas.
//!
//! `list` says what exists and what each one is for. `render` writes the
//! personas a session dispatches, from the templates that are authored.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use homma_core::Config;
use homma_org::agents::Personas;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Where the corpus is authored, relative to the workspace root.
pub const AUTHORED: &str = ".shared/agents";
/// Where the generated personas go, relative to the workspace root.
pub const GENERATED: &str = ".claude/agents";

/// The authored corpus of the workspace `cfg` resolved.
pub fn authored_dir(cfg: &Config) -> PathBuf {
    cfg.workspace.path.join(AUTHORED)
}

/// Every persona under `dir`, with the directory named in the error when one
/// of them does not read.
pub fn load(dir: &Path) -> Result<Personas> {
    Personas::load(dir).with_context(|| format!("reading the agent corpus at {}", dir.display()))
}

pub mod list {
    use super::*;

    /// Every persona and what it is for.
    #[derive(Debug, Serialize)]
    pub struct ListReport {
        pub agents: Vec<Entry>,
    }

    /// One persona.
    #[derive(Debug, Serialize)]
    pub struct Entry {
        pub name:        String,
        pub description: String,
    }

    impl HumanRender for ListReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            if self.agents.is_empty() {
                return write!(out, "no personas are authored here");
            }
            for (i, a) in self.agents.iter().enumerate() {
                if i > 0 {
                    writeln!(out)?;
                }
                writeln!(out, "  {}", a.name)?;
                write!(out, "    {}", a.description)?;
            }
            Ok(())
        }
    }

    pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
        let corpus = load(&authored_dir(cfg))?;
        emit(
            &ListReport {
                agents: corpus
                    .personas
                    .iter()
                    .map(|p| {
                        Entry {
                            name:        p.name.clone(),
                            description: p.description.clone(),
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
        pub agents:    Vec<String>,
        /// Generated personas no authored one claims.
        pub unclaimed: Vec<String>,
    }

    impl HumanRender for RenderReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            write!(
                out,
                "{} personas, from {} into {}",
                self.agents.len(),
                self.authored,
                self.generated
            )?;
            if !self.unclaimed.is_empty() {
                // Named rather than removed: on disk one is the same as a file
                // somebody put there.
                write!(
                    out,
                    "\n\nin the generated directory and authored nowhere, left alone: {}",
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
        corpus
            .render(&dst)
            .with_context(|| format!("generating personas into {}", dst.display()))?;
        let unclaimed = corpus.unclaimed(&dst)?;
        emit(
            &RenderReport {
                authored: src.display().to_string(),
                generated: dst.display().to_string(),
                agents: corpus.personas.iter().map(|p| p.name.clone()).collect(),
                unclaimed,
            },
            format,
        )?;
        Ok(())
    }
}
