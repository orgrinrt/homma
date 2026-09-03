//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma rules ...`: the workspace's own rule corpus.
//!
//! `about` answers what governs a subject, for a caller who does not know the
//! filename and would not think to look for it. `render` writes the cards a
//! session loads, from the templates that are authored.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use homma_core::Config;
use homma_org::rules::Corpus;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Where the corpus is authored, relative to the workspace root.
const AUTHORED: &str = ".shared/rules";
/// Where the generated cards go, relative to the workspace root.
const GENERATED: &str = ".claude/rules";

fn authored_dir(cfg: &Config) -> PathBuf {
    cfg.workspace.path.join(AUTHORED)
}

fn load(dir: &Path) -> Result<Corpus> {
    Corpus::load(dir).with_context(|| format!("reading the rule corpus at {}", dir.display()))
}

pub mod about {
    use super::*;

    /// The rules that answer a subject.
    #[derive(Debug, Serialize)]
    pub struct AboutReport {
        pub query: String,
        pub hits:  Vec<Hit>,
    }

    /// One rule that answers, and how much of the query it answered.
    #[derive(Debug, Serialize)]
    pub struct Hit {
        pub name:    String,
        pub fires:   String,
        pub topics:  Vec<String>,
        pub matched: usize,
    }

    impl HumanRender for AboutReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            if self.hits.is_empty() {
                // Named as a fact about the corpus rather than about the
                // question. A rule may govern this and simply not say so in its
                // topics, and reporting "nothing governs it" would be a claim
                // the search cannot support.
                return write!(
                    out,
                    "no rule declares a topic matching `{}`.\nthat is a fact about the topics, \
                     not about whether anything governs this: try another wording, or \
                     `rules find` over the bodies.",
                    self.query
                );
            }
            writeln!(out, "rules about `{}`:", self.query)?;
            for h in &self.hits {
                writeln!(out)?;
                writeln!(out, "  {}", h.name)?;
                writeln!(out, "    fires {}", h.fires)?;
                write!(out, "    topics: {}", h.topics.join(", "))?;
            }
            Ok(())
        }
    }

    pub fn run(cfg: &Config, query: &str, format: OutputFormat) -> Result<()> {
        let corpus = load(&authored_dir(cfg))?;
        let hits = corpus
            .about(query)
            .into_iter()
            .map(|(r, matched)| {
                Hit {
                    name: r.name.clone(),
                    fires: r.meta.fires.clone(),
                    topics: r.meta.topics.clone(),
                    matched,
                }
            })
            .collect();
        emit(
            &AboutReport {
                query: query.to_string(),
                hits,
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
        pub cards:     Vec<String>,
    }

    impl HumanRender for RenderReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            write!(
                out,
                "{} cards from {} into {}",
                self.cards.len(),
                self.authored,
                self.generated
            )
        }
    }

    pub fn run(cfg: &Config, format: OutputFormat) -> Result<()> {
        let src = authored_dir(cfg);
        let dst = cfg.workspace.path.join(GENERATED);
        let corpus = load(&src)?;
        let written = corpus
            .render_cards(&dst)
            .with_context(|| format!("generating cards into {}", dst.display()))?;
        emit(
            &RenderReport {
                authored:  src.display().to_string(),
                generated: dst.display().to_string(),
                cards:     written
                    .iter()
                    .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                    .collect(),
            },
            format,
        )?;
        Ok(())
    }
}
