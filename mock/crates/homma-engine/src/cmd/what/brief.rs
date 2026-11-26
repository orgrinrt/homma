//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma what --brief <file>`: what bears on a unit of work, picked from the work's own text.
//!
//! The candidates are `what`'s ranking over the work's words with a floor of two words matched. Their
//! own text goes to the summarising pass, each opened by a heading line, and the answer is kept only
//! where every quoted span is a whole line its place names, and printed within a byte cap. Where the pass is off or fails,
//! the brief is the candidates themselves and the reason there is no extract.

use std::fmt::Write as _;

use homma_api::SummaryConfig;
use serde::Serialize;

use super::brief_render::fit;
use super::doc::{Doc, Kind};
use super::vocab::Vocabulary;
use super::{rank, text};
use crate::cmd::ask::cache::Cache;
use crate::cmd::summary::{self, BYTES_PER_TOKEN, Budget, OneShot, RULES};

/// How many candidates a brief reads where `--limit` is not given.
pub const LIMIT: usize = 12;

/// The words a candidate must match of a work's text.
const NEED: usize = 2;

/// The most a printed brief is, in bytes.
pub const BYTES: usize = 3000;

/// The most quotes one thing keeps.
pub const QUOTES: usize = 3;

/// The brief the pass reads the candidates under, before the work's text.
pub const BRIEF: &str = "the rules, skills and tools that bear on this work, each named once at the \
line of its === heading with one sentence on why it applies, then each line of it that applies \
with its words copied exactly in double quotes; nothing that does not bear on the work. The work:";

/// One thing the brief names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Thing {
    pub kind:    Kind,
    pub name:    String,
    pub command: String,
    /// Why it applies, as the answer says it at the thing's heading, or the thing's summary where
    /// there is no extract.
    pub why:     Option<String>,
    /// Its lines that apply, each checked to be a whole line of its text, three at most.
    pub quotes:  Vec<String>,
}

/// A brief of a work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Brief {
    pub things:  Vec<Thing>,
    /// Lines of the answer left out: a quote that is not a whole line of its source, a line under
    /// a heading with no quote, a reason after the first, a quote past a thing's three, or no
    /// place in the content.
    pub dropped: usize,
    /// Things left out to keep the printed brief within [`BYTES`].
    pub left:    usize,
    /// Why there is no extract, where there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note:    Option<String>,
    /// The line the work's text was cut after to fit the pass, where it was cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cut:     Option<usize>,
    /// Whether it was given from the cache.
    pub cached:  bool,
}

/// The candidates for `work`, best first, at most `limit`.
#[must_use]
pub fn candidates(docs: &[Doc], work: &str, limit: usize, vocabulary: &Vocabulary) -> Vec<usize> {
    let mut terms: Vec<String> = text::pairs(work).into_iter().map(|(_, s)| s).collect();
    terms.sort();
    terms.dedup();
    let mut hits = rank::rank_with(docs, &terms, vocabulary, NEED.min(terms.len()));
    hits.truncate(limit);
    hits.into_iter().map(|h| h.doc).collect()
}

/// A thing's own text: what its source holds in full, or else its summary, its verbs and the rest
/// it says about itself.
#[must_use]
pub fn text_of(d: &Doc) -> String {
    if !d.text.trim().is_empty() {
        return d.text.clone();
    }
    let mut out = d.summary.clone();
    for v in &d.verbs {
        let _ = write!(out, "\n{}: {}", v.command, v.summary);
    }
    if !d.body.trim().is_empty() {
        let _ = write!(out, "\n{}", d.body);
    }
    out
}

/// The content the pass reads, and the line each candidate's heading is on.
struct Content {
    text:  String,
    lines: Vec<String>,
    heads: Vec<(usize, usize)>,
}

fn content(docs: &[Doc], cands: &[usize]) -> Content {
    let mut lines: Vec<String> = Vec::new();
    let mut heads = Vec::new();
    for &i in cands {
        let d = &docs[i];
        lines.push(format!(
            "=== {} {}: {} ===",
            d.kind.label(),
            d.name,
            d.command
        ));
        heads.push((lines.len(), i));
        lines.extend(text_of(d).lines().map(str::to_string));
    }
    Content {
        text: lines.join("\n"),
        lines,
        heads,
    }
}

/// The work's text as far as it fits the pass's budget for its rules, and the line it was cut
/// after where it was.
fn fitted(work: &str) -> (String, Option<usize>) {
    let room = Budget::DEFAULT
        .system
        .saturating_mul(BYTES_PER_TOKEN)
        .saturating_sub(RULES.len() + BRIEF.len() + 2);
    if work.trim().len() <= room {
        return (work.trim().to_string(), None);
    }
    let mut kept = String::new();
    let mut n = 0;
    for line in work.trim().lines() {
        if kept.len() + line.len() + 1 > room {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
        n += 1;
    }
    (kept, Some(n))
}

fn fold(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A line as a quote of it is compared: its whitespace folded, its leading list, heading and quote
/// marks and its emphasis marks set aside.
fn bare(s: &str) -> String {
    let lead = s.trim_start_matches(|c: char| {
        matches!(c, '#' | '-' | '*' | '+' | '>') || c.is_whitespace()
    });
    fold(&lead.replace("**", "").replace("__", ""))
}

/// Whether `span` is a whole line of `lines`, or the whole of them.
fn whole(span: &str, lines: &[String]) -> bool {
    let s = bare(span);
    !s.is_empty()
        && (lines.iter().any(|l| bare(l) == s)
            || bare(&lines.iter().map(|l| bare(l)).collect::<Vec<_>>().join(" ")) == s)
}

/// The place an answer's line starts with, `line <n>`, `line <a>-<b>` or `lines <a>-<b>`, and the
/// rest after it.
fn place(line: &str) -> Option<((usize, usize), &str)> {
    let rest = line
        .strip_prefix("lines ")
        .or_else(|| line.strip_prefix("line "))?;
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-'))?;
    let (span, rest) = rest.split_at(end);
    let (a, b) = match span.split_once('-') {
        Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
        None => {
            let n = span.parse().ok()?;
            (n, n)
        },
    };
    let rest = rest.trim_start().strip_prefix(':').unwrap_or(rest).trim();
    (a >= 1 && a <= b).then_some(((a, b), rest))
}

/// The spans of `text` in double quotes.
fn quoted(text: &str) -> Vec<&str> {
    text.split('"').skip(1).step_by(2).collect()
}

/// The answer kept where its places and quotes hold, given back by thing.
fn checked(docs: &[Doc], c: &Content, answer: &str) -> Brief {
    let mut b = Brief::default();
    for line in answer
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("- "))
    {
        let Some(((first, last), rest)) = place(line.trim()) else {
            b.dropped += 1;
            continue;
        };
        if last > c.lines.len() {
            b.dropped += 1;
            continue;
        }
        let Some(&(head, doc)) = c.heads.iter().rev().find(|(h, _)| *h <= first) else {
            b.dropped += 1;
            continue;
        };
        let spans = quoted(rest);
        let named = &c.lines[first - 1 .. last];
        if spans.iter().any(|s| !whole(s, named)) {
            b.dropped += 1;
            continue;
        }
        // a line off the heading counts only for what it quotes
        if spans.is_empty() && first != head {
            b.dropped += 1;
            continue;
        }
        let d = &docs[doc];
        let at = match b
            .things
            .iter()
            .position(|t| t.name == d.name && t.kind == d.kind)
        {
            Some(at) => at,
            None => {
                b.things.push(Thing {
                    kind:    d.kind,
                    name:    d.name.clone(),
                    command: d.command.clone(),
                    why:     None,
                    quotes:  Vec::new(),
                });
                b.things.len() - 1
            },
        };
        let thing = &mut b.things[at];
        if spans.is_empty() {
            // the heading's own words, `<kind> <name>:`, are the thing and not why
            let why = rest.split_once(": ").map_or(rest, |(_, w)| w).trim();
            if thing.why.is_some() {
                b.dropped += 1;
            } else if !why.is_empty() {
                thing.why = Some(why.to_string());
            }
        } else {
            for s in spans {
                if thing.quotes.len() < QUOTES {
                    thing.quotes.push(s.to_string());
                } else {
                    b.dropped += 1;
                }
            }
        }
    }
    b
}

/// The candidates as a query would print them, with why there is no extract.
fn listed(docs: &[Doc], cands: &[usize], note: String) -> Brief {
    Brief {
        things: cands
            .iter()
            .map(|&i| {
                Thing {
                    kind:    docs[i].kind,
                    name:    docs[i].name.clone(),
                    command: docs[i].command.clone(),
                    why:     Some(docs[i].summary.clone()),
                    quotes:  Vec::new(),
                }
            })
            .collect(),
        note: Some(note),
        ..Brief::default()
    }
}

/// The key a brief is kept under: the work, both rules, what answers, and each candidate's name
/// and the digest of its text.
fn key(settings: &SummaryConfig, docs: &[Doc], cands: &[usize], work: &str) -> String {
    let texts: Vec<String> = cands
        .iter()
        .map(|&i| {
            format!(
                "{} {} {}",
                docs[i].kind.label(),
                docs[i].name,
                summary::version_of(&[&text_of(&docs[i])])
            )
        })
        .collect();
    let rules = summary::rules_version();
    let own = summary::version_of(&[BRIEF]);
    let mut parts: Vec<&str> = vec![work, &rules, &own, &settings.command, &settings.model];
    parts.extend(texts.iter().map(String::as_str));
    summary::version_of(&parts)
}

/// The brief of `work` over the candidates `cands` of `docs`, through `model`, kept in `cache`
/// where one is given.
#[must_use]
pub fn brief<M: OneShot>(
    model: &M,
    settings: &SummaryConfig,
    cache: Option<&Cache>,
    docs: &[Doc],
    cands: &[usize],
    work: &str,
) -> Brief {
    if cands.is_empty() {
        return Brief::default();
    }
    let (work, cut) = fitted(work);
    let c = content(docs, cands);
    let k = key(settings, docs, cands, &work);
    if settings.enabled {
        if let Some(kept) = cache.and_then(|ch| ch.answer(&k)) {
            return fit(Brief {
                cut,
                cached: true,
                ..checked(docs, &c, &kept)
            });
        }
    }
    let pass = summary::pass_with(settings, model, &format!("{BRIEF}\n{work}"), &c.text);
    let Some(answer) = pass.summary else {
        let note = pass
            .note
            .unwrap_or_else(|| "summary: no summary".to_string());
        return fit(Brief {
            cut,
            ..listed(docs, cands, note)
        });
    };
    if let Some(ch) = cache {
        if let Err(why) = ch.keep_answer(&k, &answer) {
            eprintln!("what: the brief was not kept: {why}");
        }
    }
    fit(Brief {
        cut,
        ..checked(docs, &c, &answer)
    })
}

/// The kinds a brief reads: those asked for, never the mockspace commands, which have nothing to
/// quote.
fn kinds_read(kinds: &[Kind]) -> Vec<Kind> {
    [Kind::Tool, Kind::Script, Kind::Verb, Kind::Skill, Kind::Rule]
        .into_iter()
        .filter(|k| super::doc::wanted(kinds, *k))
        .collect()
}

/// The brief of `work` over the workspace at `root`, kept and logged under its git directory.
///
/// # Errors
/// A `[what]`, `[summary]` or `[ask]` table that does not read.
pub fn of_workspace(
    root: &std::path::Path,
    cfg: Option<&homma_core::Config>,
    work: &str,
    limit: usize,
    kinds: &[Kind],
) -> anyhow::Result<Brief> {
    use anyhow::Context as _;
    let vocabulary = Vocabulary::of(cfg)?;
    let (scope, _) = homma_core::scope::Scope::load(root);
    let g = super::gather(root, &kinds_read(kinds), true, &scope);
    let cands = candidates(&g.docs, work, limit, &vocabulary);
    let settings = match cfg {
        Some(c) => summary::settings(c)?,
        None => SummaryConfig::default(),
    };
    let kept: homma_api::AskConfig = match cfg.and_then(|c| c.ask.as_ref()) {
        Some(v) => v.clone().try_into().context("reading `[ask]`")?,
        None => homma_api::AskConfig::default(),
    };
    let dir = crate::cmd::util::homma_dir(root, "brief").ok();
    let cache = dir.clone().map(Cache::at);
    if let Some(c) = &cache {
        c.sweep(
            std::time::SystemTime::now(),
            std::time::Duration::from_secs(kept.cache_expire_hours.saturating_mul(3600)),
            kept.cache_max_bytes,
        );
    }
    let model = summary::call::Command::from(&settings);
    let b = brief(&model, &settings, cache.as_ref(), &g.docs, &cands, work);
    if let Some(d) = dir {
        log(&d.with_extension("log"), work, &b);
    }
    Ok(b)
}

/// One line of the log per brief made: when, the work's digest, what it named and what it left out.
fn log(path: &std::path::Path, work: &str, b: &Brief) {
    use std::io::Write as _;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let named: Vec<String> = b
        .things
        .iter()
        .map(|t| format!("{} {}", t.kind.label(), t.name))
        .collect();
    let line = serde_json::json!({
        "at": at,
        "work": summary::version_of(&[work]),
        "named": named,
        "dropped": b.dropped,
        "cached": b.cached,
        "extract": b.note.is_none(),
    });
    let wrote = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = wrote {
        eprintln!("what: the brief was not logged at {}: {e}", path.display());
    }
}

/// `homma what --brief <file>`: the brief printed, exit 1 where nothing bears on the work.
///
/// # Errors
/// The work cannot be read, a table does not read, or the brief cannot be written.
pub fn run(
    root: &std::path::Path,
    cfg: Option<&homma_core::Config>,
    file: &std::path::Path,
    limit: usize,
    kinds: &[Kind],
    format: crate::cli::OutputFormat,
) -> anyhow::Result<crate::cmd::Outcome> {
    use anyhow::Context as _;
    let work = if file == std::path::Path::new("-") {
        crate::cmd::ask::read_input(std::io::stdin().lock()).context("reading stdin")?
    } else {
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?
    };
    let b = of_workspace(root, cfg, &work, limit, kinds)?;
    crate::output::emit(&b, format)?;
    Ok(if b.things.is_empty() {
        crate::cmd::Outcome::ReportedFailure
    } else {
        crate::cmd::Outcome::Ok
    })
}
