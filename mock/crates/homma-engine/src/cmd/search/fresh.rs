//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma search --fresh [<days>]`: everything that changed in the last days, across the same
//! indexes a question reaches.
//!
//! The search with a filter set, not a second one: each place is opened and brought current by the
//! question's own [`open_current`] and said by its own [`index_out`]; a repository index is listed
//! by the index crate's `RepoIndex::newest` over its window filter, `filter::Changed`. What is
//! homma's is the merge of the lists by day, which the crate's layer does not yet do across
//! indexes, and the printing.

use std::time::SystemTime;

use homma_api::SummaryConfig;
use muisti::layer::{Answered, Index, Member};
use muisti::repo::RepoHit;
use muisti::repo::filter::Changed;

use super::discover::Place;
use super::{HitOut, SearchOut, date_of, index_out, open_current, summarised, today};
use crate::cmd::summary::OneShot;

/// The words of a corpus's line: it keeps no days, so nothing of it is listed.
pub const NO_DAYS: &str = "keeps no days, so what changed in it cannot be listed";

/// [`list`], and where `summarise` is given, its hits through the pass.
pub fn answer<M: OneShot>(
    places: &[Place],
    days: u32,
    limit: Option<usize>,
    summarise: Option<(&SummaryConfig, &M)>,
    now: SystemTime,
) -> SearchOut {
    summarised(list(places, days, limit, now), summarise)
}

/// Everything the indexes of `places` hold that changed in the last `days` days counting the day
/// of `now`, newest first, at most `limit` where one is given; and a line per place.
pub fn list(places: &[Place], days: u32, limit: Option<usize>, now: SystemTime) -> SearchOut {
    let today = today(now);
    let since = today - i64::from(days.max(1)) + 1;
    // Each listing keeps its place's order and its own, so a sort that is stable on the day alone
    // leaves ties in place order and then the index's.
    let mut listed: Vec<(i64, HitOut)> = Vec::new();
    let mut indexes = Vec::with_capacity(places.len());
    for p in places {
        let (opened, how) = open_current(p);
        let answered = opened.map_err(|e| e.to_string()).and_then(|index| {
            match &index {
                Index::Repository(repo) => {
                    let window = Changed::within(repo, Some(since), Some(today))
                        .map_err(|e| e.to_string())?;
                    let report = repo.newest(&window, usize::MAX, today);
                    for (at, h) in report.hits.into_iter().enumerate() {
                        listed.push((
                            h.changed.unwrap_or(i64::MIN),
                            hit(&p.name, at + 1, h, today),
                        ));
                    }
                    Ok(Answered {
                        documents: report.indexed,
                        model:     repo.model().to_string(),
                        meaning:   false,
                        caveats:   report.caveats,
                    })
                },
                Index::Corpus(_) => Err(NO_DAYS.to_string()),
            }
        });
        let m = Member {
            name: p.name.clone(),
            answered,
        };
        indexes.push(index_out(m, p, how, now));
    }
    listed.sort_by(|a, b| b.0.cmp(&a.0));
    let mut hits: Vec<HitOut> = listed.into_iter().map(|(_, h)| h).collect();
    if let Some(n) = limit {
        hits.truncate(n);
    }
    SearchOut {
        question: String::new(),
        window: Some(days),
        hits,
        indexes,
        summary: None,
        summary_note: None,
    }
}

/// One listed document as printed, its date the last day it changed.
fn hit(index: &str, rank: usize, h: RepoHit, today: i64) -> HitOut {
    HitOut {
        index: index.to_string(),
        path: h.path,
        line: h.line,
        class: Some(h.class.label().to_string()),
        date: h.changed.and_then(date_of),
        age: h.changed.map(|d| age(today - d)),
        excerpt: h.excerpt,
        rank,
        score: h.score,
        copies: h.copies,
    }
}

/// So many days before today, in words.
#[must_use]
pub fn age(days: i64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        n => format!("{n} days ago"),
    }
}
