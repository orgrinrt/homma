//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The rule corpus: what is authored, what is generated, and finding a rule
//! nobody remembers the name of.
//!
//! A workspace's rules are injected into every session it runs, sub-agents
//! included, so their size is paid before any work starts. Each is authored as
//! two templates under `.shared/rules/`:
//!
//! * `<name>.full.md.tmpl`, carrying the meta as frontmatter and the whole
//!   reasoning as its body. Fetched when a session is already in its domain.
//! * `<name>.card.md.tmpl`, the always-loaded form, rendered against that same
//!   meta so a trigger written once appears in both.
//!
//! The card a session actually loads is generated into `.claude/rules/` and is
//! never edited by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use homma_api::rule::{self, MetaError, RuleMeta};
use mockspace_template::template::TemplateEnv;
use serde::Serialize;

/// Suffix of the authored elaboration.
const FULL_SUFFIX: &str = ".full.md.tmpl";
/// Suffix of the authored card.
const CARD_SUFFIX: &str = ".card.md.tmpl";

/// One rule, as authored.
#[derive(Debug, Clone)]
pub struct Rule {
    /// The rule's name, which is its filename without either suffix.
    pub name: String,
    /// What it declares about itself.
    pub meta: RuleMeta,
    /// The elaboration's body, frontmatter already removed.
    pub body: String,
    /// The card template's source, when one was authored beside it.
    pub card: Option<String>,
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
    /// An elaboration with no card beside it, which is unreachable: nothing
    /// would ever be generated for it and no session would see the rule.
    NoCard {
        name: String,
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
            Self::NoCard {
                name,
            } => {
                write!(
                    f,
                    "{name}: an elaboration with no `{name}{CARD_SUFFIX}` beside it, so no card \
                     would be generated and no session would ever see the rule"
                )
            },
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
/// The meta plus the elaboration's body, so a card can quote as much or as
/// little of the rule as it needs to and a rule that has to carry its whole
/// reasoning in the card can say `{{ body }}`.
#[derive(Debug, Serialize)]
struct Ctx<'a> {
    name:   &'a str,
    topics: &'a [String],
    fires:  &'a str,
    kind:   String,
    paths:  &'a [String],
    body:   &'a str,
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

        // Gathered by name first, so a card is paired with its elaboration
        // whichever order the directory happens to list them in.
        let mut fulls: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut cards: BTreeMap<String, PathBuf> = BTreeMap::new();
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
            if let Some(name) = file.strip_suffix(FULL_SUFFIX) {
                fulls.insert(name.to_string(), path);
            } else if let Some(name) = file.strip_suffix(CARD_SUFFIX) {
                cards.insert(name.to_string(), path);
            }
        }

        let mut rules = Vec::with_capacity(fulls.len());
        for (name, path) in fulls {
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
            let card = match cards.get(&name) {
                Some(p) => {
                    Some(fs::read_to_string(p).map_err(|cause| {
                        CorpusError::Unreadable {
                            path: p.clone(),
                            cause,
                        }
                    })?)
                },
                None => None,
            };
            rules.push(Rule {
                name,
                meta: parsed.meta,
                body: parsed.body,
                card,
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
            let Some(card) = r.card.as_deref() else {
                return Err(CorpusError::NoCard {
                    name: r.name.clone(),
                });
            };
            let out = self.render_one(r, card)?;
            let path = dst.join(format!("{}.md", r.name));
            fs::write(&path, out).map_err(|cause| {
                CorpusError::Write {
                    path: path.clone(),
                    cause,
                }
            })?;
            written.push(path);
        }
        Ok(written)
    }

    /// Render one card's source against its rule's meta.
    pub fn render_one(&self, r: &Rule, card: &str) -> Result<String, CorpusError> {
        // A fresh environment per render rather than one shared: strict
        // undefined behaviour is the point, and a shared registry would let a
        // template resolve a name some other rule happened to define.
        let env = TemplateEnv::new();
        let ctx = Ctx {
            name:   &r.name,
            topics: &r.meta.topics,
            fires:  &r.meta.fires,
            kind:   r.meta.kind.to_string(),
            paths:  &r.meta.paths,
            body:   &r.body,
        };
        env.render_str(card, &ctx).map_err(|e| {
            CorpusError::Render {
                path:   PathBuf::from(format!("{}{CARD_SUFFIX}", r.name)),
                reason: e.to_string(),
            }
        })
    }
}
