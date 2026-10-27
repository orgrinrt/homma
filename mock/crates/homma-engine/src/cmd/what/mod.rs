//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma what <words>`: which tool, script, verb, skill or rule does a thing.
//!
//! The words are what an agent wants to do, in its own words. The answer is the things that do it, each
//! with the exact command, a one line summary and the words that made it match, the things that sit
//! beside the best of them, and a footer saying what could not be searched for want of a descriptor.
//! The sources are read where they sit and nothing is copied into an index: [`sources`] reads them,
//! [`text`] and [`rank`] match, and this module puts the answer together and prints it.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::Outcome;
use crate::output::{HumanRender, emit};

pub mod doc;
pub mod rank;
pub mod sources;
pub mod text;

use doc::{Doc, Gap, Gathered, Kind, wanted};

#[cfg(test)]
mod accept_tests;
#[cfg(test)]
mod rank_tests;
#[cfg(test)]
mod sources_tests;
#[cfg(test)]
mod text_tests;

/// How many related things are named under an answer, and under how many answers.
const RELATED_EACH: usize = 3;
const RELATED_OF: usize = 3;

/// One answer.
#[derive(Debug, Serialize)]
pub struct HitOut {
    pub kind:    Kind,
    pub name:    String,
    /// The exact command: the matched verb's where one matched best, the thing's own otherwise.
    pub command: String,
    pub summary: String,
    /// The verb that matched best.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verb:    Option<String>,
    pub score:   f64,
    pub matched: Vec<MatchOut>,
    pub related: Vec<RelatedOut>,
}

/// One word of the query and where it was found.
#[derive(Debug, Serialize)]
pub struct MatchOut {
    /// The word as typed.
    pub word:  String,
    /// The word it matched, as the thing spells it after reduction.
    pub found: String,
    /// `=` the same word, `~syn~` a synonym, `~pre~` a prefix, `~typo~` a misspelling.
    pub how:   &'static str,
    pub field: rank::Field,
}

/// A thing that sits beside an answer.
#[derive(Debug, Serialize)]
pub struct RelatedOut {
    pub kind:    Kind,
    pub name:    String,
    pub command: String,
}

/// What `homma what` answers.
#[derive(Debug, Serialize)]
pub struct Answer {
    pub query: String,
    /// The words of the query that were looked for, the filler dropped.
    pub terms: Vec<String>,
    /// How many things were read.
    pub read:  usize,
    pub hits:  Vec<HitOut>,
    pub gaps:  Vec<Gap>,
}

/// The answer to `query` over what was read. Pure: nothing here reads the disk.
pub fn answer(docs: &[Doc], gaps: Vec<Gap>, query: &str, limit: usize) -> Answer {
    let pairs = text::pairs(query);
    let stems: Vec<String> = pairs.iter().map(|(_, s)| s.clone()).collect();
    let raw = |stem: &str| {
        pairs
            .iter()
            .find(|(_, s)| s == stem)
            .map(|(w, _)| w.clone())
            .unwrap_or_default()
    };
    let mut ranked = rank::rank(docs, &stems);
    ranked.truncate(limit);
    let rel = rank::related(docs, &ranked, RELATED_OF, RELATED_EACH);
    let hits = ranked
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let d = &docs[h.doc];
            let verb = h.verb.map(|v| &d.verbs[v]);
            HitOut {
                kind:    d.kind,
                name:    d.name.clone(),
                command: verb.map_or_else(|| d.command.clone(), |v| v.command.clone()),
                summary: d.summary.clone(),
                verb:    verb.map(|v| v.name.clone()),
                score:   (h.score * 10.0).round() / 10.0,
                matched: h
                    .matches
                    .iter()
                    .map(|m| {
                        MatchOut {
                            word:  raw(&m.term),
                            found: m.token.clone(),
                            how:   m.grade.mark(),
                            field: m.field,
                        }
                    })
                    .collect(),
                related: rel
                    .get(i)
                    .map(|r| {
                        r.iter()
                            .map(|&j| {
                                RelatedOut {
                                    kind:    docs[j].kind,
                                    name:    docs[j].name.clone(),
                                    command: docs[j].command.clone(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    Answer {
        query: query.to_string(),
        terms: stems,
        read: docs.len(),
        hits,
        gaps,
    }
}

impl HumanRender for Answer {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.hits.is_empty() {
            write!(out, "nothing matches `{}`", self.query)?;
            if self.terms.is_empty() {
                write!(
                    out,
                    " (every word of it is filler: say what it is you want to do)"
                )?;
            }
            write!(out, " among {} things read", self.read)?;
        }
        for (i, h) in self.hits.iter().enumerate() {
            if i > 0 {
                writeln!(out)?;
            }
            writeln!(out, "{:>2}. {:<6} {}", i + 1, h.kind.label(), h.command)?;
            writeln!(out, "    {}", h.summary)?;
            let m: Vec<String> = h
                .matched
                .iter()
                .map(|m| {
                    if m.how == "=" {
                        format!("{} ({})", m.word, m.field.label())
                    } else {
                        format!("{}{}{} ({})", m.word, m.how, m.found, m.field.label())
                    }
                })
                .collect();
            write!(out, "    matched: {}", m.join(", "))?;
            if !h.related.is_empty() {
                let r: Vec<String> = h
                    .related
                    .iter()
                    .map(|r| format!("{} {}", r.kind.label(), r.command))
                    .collect();
                write!(out, "\n    related: {}", r.join("; "))?;
            }
        }
        for g in &self.gaps {
            write!(out, "\nnot searched: {}", g.what)?;
        }
        Ok(())
    }
}

/// Everything the wanted kinds are read from under `root`.
pub fn gather(root: &Path, kinds: &[Kind], no_mock: bool, scope: &[String]) -> Gathered {
    let mut all = Gathered::default();
    let mut add = |g: Gathered| {
        all.docs.extend(g.docs);
        all.gaps.extend(g.gaps);
    };
    if wanted(kinds, Kind::Tool) {
        #[cfg(unix)]
        add(sources::tools::gather(root));
        #[cfg(not(unix))]
        add(Gathered {
            docs: Vec::new(),
            gaps: vec![Gap {
                source: "tools",
                count:  0,
                what:   "tools are not searched: they run on renki, which builds on unix only"
                    .to_string(),
            }],
        });
    }
    if wanted(kinds, Kind::Script) {
        add(sources::scripts::gather(root));
    }
    if wanted(kinds, Kind::Verb) {
        add(sources::verbs::gather());
    }
    if wanted(kinds, Kind::Mock) && !no_mock {
        add(sources::mock::gather(root, scope));
    }
    if wanted(kinds, Kind::Skill) {
        add(sources::corpora::skills(root));
    }
    if wanted(kinds, Kind::Rule) {
        add(sources::corpora::rules(root));
    }
    all
}

/// The repositories the instance's scope names, empty where it names none or there is no local config.
fn scope(root: &Path) -> Vec<String> {
    homma_core::local::Local::load(root)
        .ok()
        .flatten()
        .map(|l| l.instance.repos)
        .unwrap_or_default()
}

/// `homma what <words>`.
///
/// # Errors
///
/// When the answer cannot be written.
pub fn run(
    root: &Path,
    words: &[String],
    limit: usize,
    kinds: &[Kind],
    no_mock: bool,
    format: OutputFormat,
) -> Result<Outcome> {
    let g = gather(root, kinds, no_mock, &scope(root));
    let a = answer(&g.docs, g.gaps, &words.join(" "), limit);
    emit(&a, format)?;
    Ok(if a.hits.is_empty() { Outcome::ReportedFailure } else { Outcome::Ok })
}
