//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The rule corpus: what is authored, what is generated, and finding a rule
//! nobody remembers the name of.
//!
//! A workspace's rules are injected into every session it runs, sub-agents
//! included, so their size is paid before any work starts. Each is authored as
//! one template under `.shared/rules/`, `<name>.md.tmpl`: the meta as
//! frontmatter, then the card, then a marker, then the elaboration.
//!
//! **The card is a prefix of the full rule**, so nothing is written twice and
//! the two cannot drift. What a session loads is that prefix, generated into
//! `.claude/rules/` and never edited by hand; what `about` and a fetch return
//! is the whole file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use homma_api::rule::{self, MetaError, RuleMeta};
use mockspace_template::template::TemplateEnv;
use serde::Serialize;

/// Suffix of an authored rule.
const RULE_SUFFIX: &str = ".md.tmpl";

/// One rule, as authored.
#[derive(Debug, Clone)]
pub struct Rule {
    /// The rule's name, which is its filename without the suffix.
    pub name:        String,
    /// What it declares about itself.
    pub meta:        RuleMeta,
    /// The card's template source: the body up to the elaboration marker.
    pub card:        String,
    /// The elaboration's template source, empty for a rule that is all card.
    pub elaboration: String,
}

/// Everything under one `.shared/rules/` directory.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub rules: Vec<Rule>,
}

/// Why a corpus could not be read or rendered.
#[derive(Debug)]
pub enum CorpusError {
    /// The directory does not exist or could not be listed.
    Unreadable {
        path:  PathBuf,
        cause: io::Error,
    },
    /// One rule's frontmatter is wrong. Named with its file, because a
    /// refusal that does not say which of ninety files it came from is one
    /// nobody can act on.
    Meta {
        path:  PathBuf,
        cause: MetaError,
    },
    /// A card template was authored but does not render.
    Render {
        path:   PathBuf,
        reason: String,
    },
    /// Writing a generated card failed.
    Write {
        path:  PathBuf,
        cause: io::Error,
    },
}

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::Meta {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::Render {
                path,
                reason,
            } => write!(f, "{}: {reason}", path.display()),
            Self::Write {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
        }
    }
}

impl std::error::Error for CorpusError {}

/// What a template renders against.
///
/// `variant` is what lets the few lines that differ between the two forms say
/// so: a card ends by naming the fetch, and that sentence is noise in the full
/// rule, where the reader is already past it.
#[derive(Debug, Serialize)]
struct Ctx<'a> {
    name:    &'a str,
    topics:  &'a [String],
    fires:   &'a str,
    kind:    String,
    paths:   &'a [String],
    variant: &'a str,
}

impl Corpus {
    /// Read every rule under `dir`.
    ///
    /// Refuses on the first unreadable meta rather than skipping it. A corpus
    /// that silently drops the files it could not parse reports a smaller,
    /// healthier corpus than the one on disk, which is the shape that passes
    /// every check and governs nothing.
    pub fn load(dir: &Path) -> Result<Self, CorpusError> {
        let entries = fs::read_dir(dir).map_err(|cause| {
            CorpusError::Unreadable {
                path: dir.to_path_buf(),
                cause,
            }
        })?;

        // Sorted by name, so the order the corpus is walked in does not depend
        // on the order the filesystem happens to list it in.
        let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in entries {
            let entry = entry.map_err(|cause| {
                CorpusError::Unreadable {
                    path: dir.to_path_buf(),
                    cause,
                }
            })?;
            let path = entry.path();
            let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if let Some(name) = file.strip_suffix(RULE_SUFFIX) {
                files.insert(name.to_string(), path);
            }
        }

        let mut rules = Vec::with_capacity(files.len());
        for (name, path) in files {
            let source = fs::read_to_string(&path).map_err(|cause| {
                CorpusError::Unreadable {
                    path: path.clone(),
                    cause,
                }
            })?;
            let parsed = rule::parse(&source).map_err(|cause| {
                CorpusError::Meta {
                    path: path.clone(),
                    cause,
                }
            })?;
            rules.push(Rule {
                name,
                meta: parsed.meta.clone(),
                card: parsed.card().to_string(),
                elaboration: parsed.elaboration().to_string(),
            });
        }
        Ok(Self {
            rules,
        })
    }

    /// The rules answering a query, best first.
    ///
    /// A rule matching nothing is left out rather than ranked last, because a
    /// list that always returns the whole corpus answers the same as no list.
    /// Ties break on name so the order is stable between runs.
    pub fn about(&self, query: &str) -> Vec<(&Rule, usize)> {
        let terms = rule::query_terms(query);
        let mut hits: Vec<(&Rule, usize)> = self
            .rules
            .iter()
            .map(|r| (r, rule::score(&r.meta, &terms)))
            .filter(|(_, score)| *score > 0)
            .collect();
        hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        hits
    }

    /// Render every card into `dst`, returning the paths written.
    ///
    /// An elaboration with no card beside it is refused rather than skipped:
    /// skipping leaves a rule authored, findable by `about`, and absent from
    /// every session that was supposed to load it.
    pub fn render_cards(&self, dst: &Path) -> Result<Vec<PathBuf>, CorpusError> {
        fs::create_dir_all(dst).map_err(|cause| {
            CorpusError::Write {
                path: dst.to_path_buf(),
                cause,
            }
        })?;

        let mut written = Vec::with_capacity(self.rules.len());
        for r in &self.rules {
            let out = self.render(r, &r.card, "card")?;
            let path = dst.join(format!("{}.md", r.name));
            // A trailing newline, because a file without one concatenates with
            // whatever is injected after it.
            fs::write(&path, format!("{}\n", out.trim_end())).map_err(|cause| {
                CorpusError::Write {
                    path: path.clone(),
                    cause,
                }
            })?;
            written.push(path);
        }
        Ok(written)
    }

    /// The whole rule, card and elaboration, rendered as one document.
    ///
    /// What a fetch returns when a session is already in the rule's domain and
    /// wants the half the card left out.
    pub fn render_full(&self, r: &Rule) -> Result<String, CorpusError> {
        let card = self.render(r, &r.card, "full")?;
        if r.elaboration.is_empty() {
            return Ok(card);
        }
        let rest = self.render(r, &r.elaboration, "full")?;
        Ok(format!("{}\n\n{}", card.trim_end(), rest.trim_end()))
    }

    /// Render one piece of a rule against its own meta.
    fn render(&self, r: &Rule, source: &str, variant: &str) -> Result<String, CorpusError> {
        // A fresh environment per render rather than one shared: strict
        // undefined behaviour is the point, and a shared registry would let a
        // template resolve a name some other rule happened to define.
        let env = TemplateEnv::new();
        let ctx = Ctx {
            name: &r.name,
            topics: &r.meta.topics,
            fires: &r.meta.fires,
            kind: r.meta.kind.to_string(),
            paths: &r.meta.paths,
            variant,
        };
        env.render_str(source, &ctx).map_err(|e| {
            CorpusError::Render {
                path:   PathBuf::from(format!("{}{RULE_SUFFIX}", r.name)),
                reason: e.to_string(),
            }
        })
    }
}
