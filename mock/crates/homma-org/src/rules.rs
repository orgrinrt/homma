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

use std::path::{Path, PathBuf};
use std::{fs, io};

use homma_api::rule::{self, MetaError, RuleMeta};
use homma_api::session::Session;
use serde::Serialize;

use crate::surface::{self, Kind, Shape, TMPL_SUFFIX as RULE_SUFFIX};

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

/// Everything under one `.shared/rules/` directory, and the principles beside it.
///
/// The principles are kept apart from the rules: nothing that compares rules
/// with each other sees one, and `about` answers with rules only. Their cards
/// are written by the same renderer under the same session binding.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub rules:      Vec<Rule>,
    /// Each named with its card prefix, which is the name its card is written as.
    pub principles: Vec<Rule>,
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
    /// A principle is not shaped as one.
    Principle {
        path:   PathBuf,
        reason: String,
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
            }
            | Self::Principle {
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

/// The `paths:` block a generated card opens with, empty for a rule with none.
///
/// **A rule's path gating is part of the card, not only of the meta.** The host
/// reads this frontmatter to decide whether the rule is injected always or only
/// against matching files, so a generation pass that renders the body alone
/// silently promotes every gated rule to always-loaded. Measured on the first
/// two gated rules that moved into the authored corpus: both became
/// always-loaded, and the only sign was the always-loaded count rising by two.
///
/// The block form rather than the inline one, because that is what the host and
/// the workspace's own budget tool already read.
fn paths_frontmatter(r: &Rule) -> String {
    if r.meta.paths.is_empty() {
        return String::new();
    }
    let mut out = String::from("---\npaths:\n");
    for p in &r.meta.paths {
        out.push_str(&format!("  - \"{p}\"\n"));
    }
    out.push_str("---\n\n");
    out
}

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
        Ok(Self {
            rules:      surface::load(&Rules, dir)?,
            principles: Vec::new(),
        })
    }

    /// Read every principle under `dir` beside the rules, refusing one with an
    /// elaboration or a first heading that is not its title.
    pub fn load_principles(
        &mut self,
        dir: &Path,
        kind: &Principles<'_>,
    ) -> Result<(), CorpusError> {
        self.principles = surface::load(kind, dir)?;
        Ok(())
    }

    /// The rules and the principles, every card this corpus writes.
    fn cards(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter().chain(&self.principles)
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
    /// Every rule has a card by construction, since the card is the file's own
    /// opening rather than a second document that could be missing.
    ///
    /// A card in `dst` that no authored rule claims is left alone and reported
    /// by [`Self::unclaimed`], the way [`crate::skills`] treats a directory
    /// nothing claims. `dst` is the always-loaded corpus, so such a card is
    /// injected into every session for as long as it is there, which is why it
    /// cannot go unreported; and on disk a card this pass wrote last time and a
    /// document somebody put there on purpose are the same file, which is why
    /// it cannot be deleted.
    ///
    /// **Only the cards the session binds are written.** A rule that excludes
    /// the session's role or place has no card in its always-loaded set, and a
    /// card of such a rule that an earlier pass wrote, for another role, is
    /// removed: the rule claims it, so it is the pass's own file and the stale
    /// copy would be injected into the session the rule says it is not for.
    pub fn render_cards(&self, dst: &Path, session: &Session) -> Result<Vec<PathBuf>, CorpusError> {
        fs::create_dir_all(dst).map_err(|cause| {
            CorpusError::Write {
                path: dst.to_path_buf(),
                cause,
            }
        })?;

        let mut written = Vec::with_capacity(self.rules.len() + self.principles.len());
        for r in self.cards() {
            let path = dst.join(format!("{}.md", r.name));
            if !session.admits(&r.meta) {
                match fs::remove_file(&path) {
                    Ok(()) => {},
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {},
                    Err(cause) => {
                        return Err(CorpusError::Write {
                            path,
                            cause,
                        });
                    },
                }
                continue;
            }
            let body = self.card_text(r)?;
            fs::write(&path, body).map_err(|cause| {
                CorpusError::Write {
                    path: path.clone(),
                    cause,
                }
            })?;
            written.push(path);
        }
        Ok(written)
    }

    /// The generated card of a rule, exactly as it is written to disk.
    ///
    /// One function for the writing and for [`Self::card_bytes`], so the size
    /// reported is the size of what a session loads and the two cannot drift.
    fn card_text(&self, r: &Rule) -> Result<String, CorpusError> {
        let out = self.render(r, &r.card, "card")?;
        // A trailing newline, because a file without one concatenates with
        // whatever is injected after it.
        Ok(format!("{}{}\n", paths_frontmatter(r), out.trim_end()))
    }

    /// How many bytes a rule's card is in a session that loads it.
    pub fn card_bytes(&self, r: &Rule) -> Result<usize, CorpusError> {
        Ok(self.card_text(r)?.len())
    }

    /// The rules and principles the session does not load.
    pub fn withheld<'a>(&'a self, session: &'a Session) -> impl Iterator<Item = &'a Rule> {
        self.cards().filter(move |r| !session.admits(&r.meta))
    }

    /// Cards in `dst` that no authored rule or principle claims.
    ///
    /// Reported rather than removed, on the same ground the skill corpus gives
    /// for an unclaimed directory: one is either a rule that has not been moved
    /// to the authored side yet or something else entirely, and this pass
    /// cannot tell which.
    ///
    /// It matters more here than there. `dst` is the always-loaded set, so a
    /// card whose rule was deleted or renamed is not a stale file sitting in a
    /// directory, it is a document that keeps being injected into every session
    /// with nothing saying so.
    pub fn unclaimed(&self, dst: &Path) -> Result<Vec<String>, CorpusError> {
        surface::unclaimed(dst, Shape::File, |name| {
            self.cards().any(|r| r.name == name)
        })
        .map_err(|cause| Rules::unreadable(dst.to_path_buf(), cause))
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
        let ctx = Ctx {
            name: &r.name,
            topics: &r.meta.topics,
            fires: &r.meta.fires,
            kind: r.meta.kind.to_string(),
            paths: &r.meta.paths,
            variant,
        };
        surface::render(source, &ctx).map_err(|reason| {
            CorpusError::Render {
                path: PathBuf::from(format!("{}{RULE_SUFFIX}", r.name)),
                reason,
            }
        })
    }
}

/// The rules as a kind: every `.md.tmpl` in the directory is one.
///
/// **Every `.md.tmpl` here is a rule, dot in its name or not.** A guard that
/// once skipped a name containing a dot, to keep an old sidecar shape from
/// loading as a rule, outlived that shape and silently dropped
/// `no-legacy-shims-pre-1.0`, whose dot is a version number: authored,
/// discoverable, and in no session at all. A file here that cannot be read is
/// refused by name rather than passed over.
struct Rules;

impl Kind for Rules {
    type Error = CorpusError;
    type Item = Rule;

    const SHAPE: Shape = Shape::File;

    fn unreadable(path: PathBuf, cause: io::Error) -> CorpusError {
        CorpusError::Unreadable {
            path,
            cause,
        }
    }

    fn read(&self, name: &str, path: &Path) -> Result<Rule, CorpusError> {
        let source = surface::read_text::<Self>(path)?;
        let parsed = rule::parse(&source).map_err(|cause| {
            CorpusError::Meta {
                path: path.to_path_buf(),
                cause,
            }
        })?;
        Ok(Rule {
            name:        name.to_string(),
            meta:        parsed.meta.clone(),
            card:        parsed.card().to_string(),
            elaboration: parsed.elaboration().to_string(),
        })
    }
}

/// The principles as a kind: a rule's frontmatter and a card, nothing after it,
/// named and titled by the configured prefixes.
#[derive(Debug, Clone, Copy)]
pub struct Principles<'a> {
    /// What a principle's card is named: this, then its name.
    pub card_prefix:  &'a str,
    /// What its first heading opens with, before its name.
    pub title_prefix: &'a str,
}

impl Kind for Principles<'_> {
    type Error = CorpusError;
    type Item = Rule;

    const SHAPE: Shape = Shape::File;

    fn unreadable(path: PathBuf, cause: io::Error) -> CorpusError {
        Rules::unreadable(path, cause)
    }

    fn read(&self, name: &str, path: &Path) -> Result<Rule, CorpusError> {
        let mut r = Rules.read(name, path)?;
        let refuse = |reason: String| {
            CorpusError::Principle {
                path: path.to_path_buf(),
                reason,
            }
        };
        if !r.elaboration.is_empty() || r.card.lines().any(|l| l.trim() == rule::ELABORATION_MARKER)
        {
            return Err(refuse(
                "carries an elaboration, and a principle is a card and nothing after it".into(),
            ));
        }
        let want = format!("# {}{name}", self.title_prefix);
        let title = r.card.lines().find(|l| l.starts_with("# ")).unwrap_or("");
        if title.trim_end() != want {
            return Err(refuse(format!(
                "its first heading is `{title}`, and a principle's is `{want}`"
            )));
        }
        r.name = format!("{}{name}", self.card_prefix);
        Ok(r)
    }
}
