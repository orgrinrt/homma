//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma rules ...`: the workspace's own rule corpus.
//!
//! `about` answers what governs a subject, for a caller who does not know the
//! filename and would not think to look for it. `render` writes the cards a
//! session loads, from the templates that are authored.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use homma_api::session::{CLOUD_VAR, Place, ROLE_FILE, Session, SessionRole};
use homma_core::Config;
use homma_org::rules::Corpus;
use serde::Serialize;

use crate::cli::{OutputFormat, SessionArgs};
use crate::cmd::skills;
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

/// The session the options and the machine describe.
///
/// An option wins over what the machine says. Without `--role` the role file
/// is read, and a file that is there and holds anything but a role is refused
/// by name, since reading it as no role would load every card for a session
/// whose file was merely misspelt. `home` and `cloud` are arguments so a test
/// is not at the mercy of the process environment.
pub(crate) fn resolve_session(
    args: &SessionArgs,
    home: Option<&Path>,
    cloud: Option<&str>,
) -> Result<Session> {
    let file = home.map(|h| h.join(ROLE_FILE));
    let text = match (&args.role, &file) {
        (None, Some(path)) => {
            match fs::read_to_string(path) {
                Ok(t) => Some(t),
                Err(e) if e.kind() == ErrorKind::NotFound => None,
                Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
            }
        },
        _ => None,
    };
    let mut session = Session::from_parts(text.as_deref(), cloud).with_context(|| {
        format!(
            "the role file {}",
            file.as_deref()
                .map_or_else(|| ROLE_FILE.into(), |p| p.display().to_string())
        )
    })?;
    if let Some(role) = args.role {
        session.role = Some(role);
    }
    if let Some(place) = args.place {
        session.place = place;
    }
    Ok(session)
}

/// The session of this process: the options, then `$HOME` and `CLAUSE_CLOUD`.
fn current_session(args: &SessionArgs) -> Result<Session> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cloud = std::env::var(CLOUD_VAR).ok();
    resolve_session(args, home.as_deref(), cloud.as_deref())
}

/// How a session reads in a report: its role, or `any` for one with none.
fn role_word(role: Option<SessionRole>) -> &'static str {
    role.map_or("any", SessionRole::word)
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
        /// The skills citing this rule, so a session already in the domain is
        /// told what to read next rather than having to know it exists.
        /// Derived, never declared: a `skills` field on a rule is a second copy
        /// of what the skill already says and would go stale on its own.
        pub skills:  Vec<String>,
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
                writeln!(out, "    topics: {}", h.topics.join(", "))?;
                if h.skills.is_empty() {
                    write!(out, "    no skill bears on it")?;
                } else {
                    write!(out, "    skills: {}", h.skills.join(", "))?;
                }
            }
            Ok(())
        }
    }

    pub fn run(cfg: &Config, query: &str, format: OutputFormat) -> Result<()> {
        let corpus = load(&authored_dir(cfg))?;
        // The skill corpus is optional here rather than required. A workspace
        // may have rules and no skills, and refusing to answer what governs a
        // subject because nothing elaborates it would be the wrong refusal.
        let skills = skills::load(&skills::authored_dir(cfg)).ok();
        let hits = corpus
            .about(query)
            .into_iter()
            .map(|(r, matched)| {
                Ok(Hit {
                    name: r.name.clone(),
                    fires: r.meta.fires.clone(),
                    topics: r.meta.topics.clone(),
                    matched,
                    skills: match &skills {
                        Some(s) => s.bearing_on(&r.name)?,
                        None => Vec::new(),
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
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
        /// The role the cards were chosen for, `any` for a session with none.
        pub role:      String,
        pub place:     Place,
        pub cards:     Vec<String>,
        /// Rules whose cards this session does not load. Discovery still
        /// answers for them.
        pub withheld:  Vec<String>,
        /// Cards in the generated tree that no authored rule claims.
        pub unclaimed: Vec<String>,
    }

    impl HumanRender for RenderReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            write!(
                out,
                "{} cards from {} into {} for {} {}",
                self.cards.len(),
                self.authored,
                self.generated,
                self.role,
                self.place
            )?;
            if !self.withheld.is_empty() {
                write!(
                    out,
                    "\n\n{} rules not for this session, no card written: {}",
                    self.withheld.len(),
                    self.withheld.join(", ")
                )?;
            }
            if !self.unclaimed.is_empty() {
                // Named rather than removed: one is either a rule not moved to
                // the authored side yet or something else entirely, and this
                // pass cannot tell which. The generated tree is the
                // always-loaded set, so an unnamed one is carried into every
                // session with nothing saying so.
                write!(
                    out,
                    "\n\nin the generated tree and authored nowhere, left alone: {}",
                    self.unclaimed.join(", ")
                )?;
            }
            Ok(())
        }
    }

    pub fn run(cfg: &Config, args: &SessionArgs, format: OutputFormat) -> Result<()> {
        let session = current_session(args)?;
        let src = authored_dir(cfg);
        let dst = cfg.workspace.path.join(GENERATED);
        let corpus = load(&src)?;
        let written = corpus
            .render_cards(&dst, &session)
            .with_context(|| format!("generating cards into {}", dst.display()))?;
        let unclaimed = corpus.unclaimed(&dst)?;
        emit(
            &RenderReport {
                authored: src.display().to_string(),
                generated: dst.display().to_string(),
                role: role_word(session.role).to_string(),
                place: session.place,
                cards: written
                    .iter()
                    .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                    .collect(),
                withheld: corpus.withheld(&session).map(|r| r.name.clone()).collect(),
                unclaimed,
            },
            format,
        )?;
        Ok(())
    }
}

pub mod load {
    use super::*;

    /// One card and what it costs a session that loads it.
    #[derive(Debug, Serialize)]
    pub struct CardSize {
        pub name:  String,
        pub bytes: usize,
    }

    /// What one kind of session loads.
    #[derive(Debug, Serialize)]
    pub struct Row {
        /// The role, `any` for a session with none, which takes every role.
        pub role:  String,
        pub place: Place,
        pub cards: usize,
        pub bytes: usize,
        /// The cards, largest first. Left out of the table over every kind of
        /// session, where it would be a page of names per row.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub names: Vec<CardSize>,
    }

    /// What the sessions load.
    #[derive(Debug, Serialize)]
    pub struct LoadReport {
        pub rows: Vec<Row>,
    }

    impl HumanRender for LoadReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            for (i, row) in self.rows.iter().enumerate() {
                if i > 0 {
                    writeln!(out)?;
                }
                write!(
                    out,
                    "{} {}: {} cards, {} bytes",
                    row.role, row.place, row.cards, row.bytes
                )?;
                for c in &row.names {
                    write!(out, "\n  {:>7}  {}", c.bytes, c.name)?;
                }
            }
            Ok(())
        }
    }

    /// The cards one session loads, counted from the card text itself.
    fn row(corpus: &Corpus, session: &Session, names: bool) -> Result<Row> {
        let mut sizes = corpus
            .admitted(session)
            .map(|r| {
                Ok(CardSize {
                    name:  r.name.clone(),
                    bytes: corpus.card_bytes(r)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let bytes = sizes.iter().map(|c| c.bytes).sum();
        let cards = sizes.len();
        sizes.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
        Ok(Row {
            role: role_word(session.role).to_string(),
            place: session.place,
            cards,
            bytes,
            names: if names { sizes } else { Vec::new() },
        })
    }

    pub fn run(cfg: &Config, args: &SessionArgs, all: bool, format: OutputFormat) -> Result<()> {
        let corpus = load(&authored_dir(cfg))?;
        let rows = if all {
            let roles = [None, Some(SessionRole::Coordinator), Some(SessionRole::Helper)];
            let mut rows = Vec::new();
            for role in roles {
                for place in Place::ALL {
                    rows.push(row(
                        &corpus,
                        &Session {
                            role,
                            place,
                        },
                        false,
                    )?);
                }
            }
            rows
        } else {
            vec![row(&corpus, &current_session(args)?, true)?]
        };
        emit(
            &LoadReport {
                rows,
            },
            format,
        )?;
        Ok(())
    }
}
