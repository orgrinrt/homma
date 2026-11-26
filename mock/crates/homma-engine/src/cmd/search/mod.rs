//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma search <words>`: one question across every index the workspace holds.
//!
//! A layer over the indexes and nothing more: each is opened through the index crate, brought
//! current by the crate's own refresh, which writes it, in [`current`], asked read only by its own
//! ranking, and the answers fused by rank there. What is homma's is which indexes exist and what
//! each is called, in [`discover`], and the printing. No member's command is run and
//! no member knows of this.

use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use homma_api::SummaryConfig;
use muisti::layer::{self, Across, Member, Question};
use muisti::repo::Words;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::Outcome;
use crate::cmd::summary::{self, OneShot, Pass};
use crate::output::{HumanRender, emit};

pub mod current;
pub mod discover;
pub mod fresh;

use current::Currency;
use discover::Place;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_chunker;
#[cfg(test)]
mod tests_current;
#[cfg(test)]
mod tests_fresh;
#[cfg(test)]
mod tests_fresh_edges;

/// One hit, as printed.
#[derive(Debug, Serialize)]
pub struct HitOut {
    /// The index it came from.
    pub index:   String,
    pub path:    String,
    pub line:    usize,
    /// Who wrote it, where the index knows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class:   Option<String>,
    /// The day it was written, where the index knows; in a listing, the last day it changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date:    Option<String>,
    /// How many days before today that was, in a listing, where the day is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age:     Option<String>,
    pub excerpt: String,
    /// Its rank in its own index's answer.
    pub rank:    usize,
    pub score:   f64,
    /// The same document in other indexes, as `index:path`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub copies:  Vec<String>,
}

/// One index, and how it took the question.
#[derive(Debug, Serialize)]
pub struct IndexOut {
    pub name:      String,
    pub file:      String,
    pub answered:  bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model:     Option<String>,
    pub meaning:   bool,
    /// How long ago the file was last written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written:   Option<String>,
    /// How current it was made before it was asked, where its file opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency:  Option<Currency>,
    /// Why it was not asked, or what it could not answer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes:     Vec<String>,
}

/// The answer.
#[derive(Debug, Serialize)]
pub struct SearchOut {
    /// The question's words; empty, and absent from the JSON, for a listing.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub question:     String,
    /// The days a listing of what changed covers, present only for a listing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window:       Option<NonZeroU32>,
    pub hits:         Vec<HitOut>,
    pub indexes:      Vec<IndexOut>,
    /// The hits summarised, printed in their place, where `--summarise` asked and there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary:      Option<String>,
    /// Why `--summarise` gave no summary, where it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_note: Option<String>,
}

impl SearchOut {
    /// The brief the hits are summarised for.
    #[must_use]
    pub fn brief(&self) -> String {
        match self.window {
            Some(n) => {
                format!(
                    "everything that changed in the last {} across the workspace's indexes: what \
                     changed where, newest first",
                    days(n)
                )
            },
            None => {
                format!(
                    "the hits of a search for \"{}\": which places answer it and what each says",
                    self.question
                )
            },
        }
    }

    /// The hits as printed, the text the pass reads.
    #[must_use]
    pub fn hits_text(&self) -> String {
        let mut text = Vec::new();
        // Writing to a vector does not fail.
        let _ = self.render_hits(&mut text);
        String::from_utf8_lossy(&text).into_owned()
    }

    /// Take what the pass gave: the summary, or the reason there is none.
    pub fn take(&mut self, pass: Pass) {
        self.summary = pass.summary;
        self.summary_note = pass.note;
    }

    fn render_hits(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.hits.is_empty() {
            match self.window {
                Some(n) => {
                    writeln!(out, "nothing in any index changed in the last {}", days(n))?;
                },
                None => writeln!(out, "nothing in any index answers `{}`", self.question)?,
            }
        }
        let width = self.hits.iter().map(|h| h.index.len()).max().unwrap_or(0);
        for h in &self.hits {
            let mut said = Vec::new();
            if let Some(c) = &h.class {
                said.push(c.clone());
            }
            if let Some(d) = &h.date {
                said.push(d.clone());
            }
            if let Some(a) = &h.age {
                said.push(a.clone());
            }
            let said =
                if said.is_empty() { String::new() } else { format!("  ({})", said.join(", ")) };
            writeln!(out, "{:width$}  {}:{}{said}", h.index, h.path, h.line)?;
            writeln!(out, "{:width$}      {}", "", h.excerpt)?;
            for c in &h.copies {
                writeln!(out, "{:width$}      also {c}", "")?;
            }
        }
        Ok(())
    }
}

/// `homma search`: discover the indexes under `root`, ask them, print the answer. Fails as a
/// reported failure when no index answered.
///
/// # Errors
/// What kept the indexes from being listed, or a name `only` gives that no index carries.
pub fn run(
    places: Vec<Place>,
    asked: &Asked,
    only: &[String],
    limit: Option<usize>,
    summarise: Option<&SummaryConfig>,
    format: OutputFormat,
) -> Result<Outcome> {
    let places = discover::narrowed(places, only)?;
    let model = summarise.map(summary::call::Command::from);
    let summarise = summarise.zip(model.as_ref());
    let now = SystemTime::now();
    let out = match asked {
        Asked::Words(words) => {
            answer(
                &places,
                &words.join(" "),
                limit.unwrap_or(QUESTION_LIMIT),
                summarise,
                now,
            )
        },
        Asked::Fresh(days) => fresh::answer(&places, *days, limit, summarise, now),
    };
    emit(&out, format)?;
    Ok(if out.indexes.iter().any(|i| i.answered) {
        Outcome::Ok
    } else {
        Outcome::ReportedFailure
    })
}

/// What a search is asked: a question in words, or what changed in the last days.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// A question, its words as given.
    Words(Vec<String>),
    /// Everything that changed in the last so many days, counting today.
    Fresh(NonZeroU32),
}

/// How many hits a question gives where `--limit` is not given.
pub const QUESTION_LIMIT: usize = 10;

/// The pass a search goes through: the `[summary]` table `settings` reads where `--summarise`
/// asked for it, whatever it says, so it says why where it is off; and for a listing, which is
/// large by design, wherever the table has it on. `settings` is read only where one of the two may
/// want it.
///
/// # Errors
/// What `settings` gives.
pub fn pass_for(
    asked: &Asked,
    summarise: bool,
    settings: impl FnOnce() -> Result<SummaryConfig>,
) -> Result<Option<SummaryConfig>> {
    let listing = matches!(asked, Asked::Fresh(_));
    if !summarise && !listing {
        return Ok(None);
    }
    Ok(Some(settings()?).filter(|s| summarise || s.enabled))
}

/// `n` days, in its number.
fn days(n: NonZeroU32) -> String {
    if n.get() == 1 { "1 day".to_string() } else { format!("{n} days") }
}

/// [`ask`], and where `summarise` is given, its hits through the pass with that table and model.
pub fn answer<M: OneShot>(
    places: &[Place],
    question: &str,
    limit: usize,
    summarise: Option<(&SummaryConfig, &M)>,
    now: SystemTime,
) -> SearchOut {
    summarised(ask(places, question, limit, now), summarise)
}

/// `out` with its hits through the pass, where `summarise` is given; a search with no hits makes no
/// call.
fn summarised<M: OneShot>(
    mut out: SearchOut,
    summarise: Option<(&SummaryConfig, &M)>,
) -> SearchOut {
    if let Some((settings, model)) = summarise {
        if out.hits.is_empty() {
            out.take(Pass::none(
                "summary: there were no hits to summarise, so no call was made".to_string(),
            ));
            return out;
        }
        out.take(summary::pass_with(
            settings,
            model,
            &out.brief(),
            &out.hits_text(),
        ));
    }
    out
}

/// Ask `question` of every place, one open at a time, and shape the answer.
pub fn ask(places: &[Place], question: &str, limit: usize, now: SystemTime) -> SearchOut {
    let q = Question {
        text: question,
        limit,
        today: today(now),
    };
    // How each place was brought current, in the order given; filled as the question reaches it,
    // so each index is refreshed, asked and dropped before the next is opened.
    let mut currencies: Vec<Opened> = Vec::with_capacity(places.len());
    let across: Across = layer::across(
        places.iter().map(|p| {
            let (opened, how) = open_current(p);
            currencies.push(how);
            (p.name.clone(), opened)
        }),
        &q,
        None::<&mut Words>,
    );
    let hits = across
        .hits
        .into_iter()
        .map(|h| {
            HitOut {
                index:   h.member,
                path:    h.passage.path,
                line:    h.passage.line,
                class:   h.passage.class.map(|c| c.label().to_string()),
                date:    h.passage.day.and_then(date_of),
                age:     None,
                excerpt: h.passage.excerpt,
                rank:    h.rank,
                score:   h.score,
                copies:  h
                    .copies
                    .into_iter()
                    .map(|(index, path)| format!("{index}:{path}"))
                    .collect(),
            }
        })
        .collect();
    let indexes = across
        .members
        .into_iter()
        .zip(places)
        .zip(currencies)
        .map(|((m, p), how)| index_out(m, p, how, now))
        .collect();
    SearchOut {
        question: question.to_string(),
        window: None,
        hits,
        indexes,
        summary: None,
        summary_note: None,
    }
}

/// How current a place was made before it was asked, where its file opened, and what the refresh
/// could not read.
pub type Opened = (Option<Currency>, Vec<String>);

/// The place's index opened and brought current, or the error that kept it shut; and how current
/// it was made. Every way a search reaches a place goes through here, so a question and a listing
/// open, refresh and say a place alike.
pub fn open_current(p: &Place) -> (Result<layer::Index, muisti::Error>, Opened) {
    let opened = match &p.shut {
        // Stands in for the place so it stays a member; its line is the reason, not this error.
        Some(why) => Err(muisti::Error::Cache(why.clone())),
        None => layer::open(&p.index),
    };
    match opened {
        Ok(mut index) => {
            let (c, caveats) = current::bring(&mut index, &mut Words, p.build.as_deref());
            (Ok(index), (Some(c), caveats))
        },
        Err(e) => {
            let moves = current::other_format(&e, p.build.as_deref());
            (Err(e), (None, moves.into_iter().collect()))
        },
    }
}

/// The line of one place: how it answered or why it did not, how current it was made, and when
/// its file was last written as of `now`.
pub fn index_out(m: Member, p: &Place, (currency, caveats): Opened, now: SystemTime) -> IndexOut {
    // A file written while the question ran is newer than `now`, and was written just now.
    let shut = p.shut.is_some();
    let written = (!shut)
        .then(|| written(&p.index))
        .flatten()
        .map(|t| ago(now.duration_since(t).unwrap_or_default()));
    let mut notes = p.notes.clone();
    match m.answered {
        Ok(a) => {
            notes.extend(caveats);
            notes.extend(a.caveats);
            IndexOut {
                name: m.name,
                file: p.index.display().to_string(),
                answered: true,
                documents: Some(a.documents),
                model: Some(a.model),
                meaning: a.meaning,
                written,
                currency,
                notes,
            }
        },
        Err(why) => {
            // A place that is shut is said in its own words; the index crate's error for it is
            // only the stand-in that keeps it a member of the question, and its prefix is not true.
            notes.push(p.shut.clone().unwrap_or(why));
            // What the refusal leaves to do follows the refusal's own words.
            notes.extend(caveats);
            // An index not there is built by its own command, never by the question.
            if let Some(build) = p
                .build
                .as_ref()
                .filter(|_| p.shut.is_none() && !p.index.exists())
            {
                notes.push(format!("built by {build}"));
            }
            IndexOut {
                name: m.name,
                file: p.index.display().to_string(),
                answered: false,
                documents: None,
                model: None,
                meaning: false,
                written,
                currency,
                notes,
            }
        },
    }
}

/// `now` as days since 1970-01-01, in UTC.
#[must_use]
pub fn today(now: SystemTime) -> i64 {
    let since_epoch = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_secs() / 86_400).unwrap_or(0)
}

/// When the index file or the log written ahead of it last changed, whichever is later.
fn written(index: &Path) -> Option<SystemTime> {
    let mut wal = index.as_os_str().to_owned();
    wal.push("-wal");
    [index.to_path_buf(), PathBuf::from(wal)]
        .iter()
        .filter_map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
        .max()
}

/// A span as the coarsest unit that says it.
fn ago(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0 .. 120 => format!("{s} s ago"),
        120 .. 7_200 => format!("{} min ago", s / 60),
        7_200 .. 172_800 => format!("{} h ago", s / 3_600),
        _ => format!("{} days ago", s / 86_400),
    }
}

/// A day since 1970-01-01 as `YYYY-MM-DD`.
fn date_of(day: i64) -> Option<String> {
    let t = jiff::Timestamp::from_second(day.checked_mul(86_400)?).ok()?;
    Some(t.to_zoned(jiff::tz::TimeZone::UTC).date().to_string())
}

impl HumanRender for SearchOut {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        match &self.summary {
            Some(s) => writeln!(out, "{s}")?,
            None => self.render_hits(out)?,
        }
        if let Some(n) = &self.summary_note {
            writeln!(out, "{n}")?;
        }
        writeln!(out)?;
        writeln!(out, "INDEXES")?;
        let width = self.indexes.iter().map(|i| i.name.len()).max().unwrap_or(0);
        for i in &self.indexes {
            let mut when = i
                .written
                .as_deref()
                .map(|w| format!(", written {w}"))
                .unwrap_or_default();
            if let Some(c) = &i.currency {
                when.push_str(", ");
                when.push_str(&c.said());
            }
            if i.answered {
                let by = match (self.window, i.meaning) {
                    (Some(_), _) => "listed by the days they changed",
                    (None, true) => "asked by words and meaning",
                    (None, false) => "asked by words",
                };
                write!(
                    out,
                    "  {:width$}  {} documents, `{}` vectors, {by}{when}",
                    i.name,
                    i.documents.unwrap_or(0),
                    i.model.as_deref().unwrap_or("")
                )?;
            } else {
                write!(out, "  {:width$}  not asked{when}", i.name)?;
            }
            for n in &i.notes {
                write!(out, "\n  {:width$}    {n}", "")?;
            }
            writeln!(out)?;
        }
        Ok(())
    }
}
