//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Scoring the things read against a query's terms, and naming what sits beside a match.
//!
//! A term is looked for in seven places, each with its own weight, and the best way it matches in the
//! best place is what it is worth. A term is worth more the fewer things carry it, so a word that names
//! half the tooling does not decide an answer. A thing scores the sum over the terms, scaled by how many
//! of them it matched, and a thing that matched too few of them is not an answer.

use serde::Serialize;

use super::doc::{Doc, Kind};
use super::text::{self, Grade};

/// The places a term is looked for, and what a match there is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    Name,
    Tag,
    VerbName,
    Summary,
    VerbSummary,
    Body,
    Place,
}

impl Field {
    const fn weight(self) -> f64 {
        match self {
            Self::Name => 6.0,
            Self::Tag => 4.0,
            Self::VerbName => 3.5,
            Self::Summary => 2.5,
            Self::VerbSummary => 2.0,
            Self::Body => 1.0,
            Self::Place => 1.5,
        }
    }

    /// The word printed beside a match.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Tag => "tag",
            Self::VerbName => "verb",
            Self::Summary => "summary",
            Self::VerbSummary => "verb summary",
            Self::Body => "description",
            Self::Place => "place",
        }
    }
}

/// A thing lower than this share of the best score is dropped.
const CUT: f64 = 0.2;
/// How close two things must be, as the cosine of their weighted words, to be named beside each other.
pub const CLOSE: f64 = 0.225;

/// How one term matched in one thing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Match {
    pub term:  String,
    /// The word of the thing it matched.
    pub token: String,
    pub field: Field,
    #[serde(skip)]
    pub grade: Grade,
    #[serde(skip)]
    pub value: f64,
    /// What this term is worth in each verb it matched, apart from the field that won: the verb's index and the
    /// best value. The thing's own bare command is not among them.
    #[serde(skip)]
    pub verbs: Vec<(usize, f64)>,
}

/// A thing that answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub doc:     usize,
    pub score:   f64,
    pub matches: Vec<Match>,
    /// The verb that matched best, where one did.
    pub verb:    Option<usize>,
}

/// The tokens of a thing, by place.
struct Tokens {
    name:         Vec<String>,
    tags:         Vec<String>,
    verb_names:   Vec<(usize, String)>,
    summary:      Vec<String>,
    verb_summary: Vec<(usize, String)>,
    body:         Vec<String>,
    place:        Vec<String>,
    /// Which verbs are the thing's own bare command, which is never the verb an answer names.
    bare:         Vec<bool>,
}

fn stems(text: &str) -> Vec<String> {
    text::words(text)
        .into_iter()
        .filter(|w| !text::is_filler(w))
        .map(|w| text::stem(&w))
        .collect()
}

fn tokens(d: &Doc) -> Tokens {
    let mut verb_names = Vec::new();
    let mut verb_summary = Vec::new();
    for (i, v) in d.verbs.iter().enumerate() {
        // A worked invocation says what the verb does the way its summary does, and its words are not names.
        let into = if v.example { &mut verb_summary } else { &mut verb_names };
        into.extend(stems(&v.name).into_iter().map(|s| (i, s)));
        verb_summary.extend(stems(&v.summary).into_iter().map(|s| (i, s)));
    }
    // A rule is named by a sentence, which says what it is about the way a summary does.
    let (name, extra) = if d.kind == Kind::Rule {
        (Vec::new(), stems(&d.name))
    } else {
        (stems(&d.name), Vec::new())
    };
    Tokens {
        name,
        tags: d.tags.iter().flat_map(|t| stems(t)).collect(),
        verb_names,
        summary: stems(&d.summary).into_iter().chain(extra).collect(),
        body: stems(&d.body),
        verb_summary,
        place: d.place.iter().flat_map(|t| stems(t)).collect(),
        bare: d.verbs.iter().map(|v| v.command == d.command).collect(),
    }
}

/// The best way `term` matches in `t`, if it does.
fn best(term: &str, t: &Tokens) -> Option<Match> {
    let mut found: Option<Match> = None;
    let mut per_verb: Vec<(usize, f64)> = Vec::new();
    let mut offer = |field: Field, token: &str, verb: Option<usize>| {
        if let Some(g) = text::grade(term, token) {
            let value = field.weight() * g.worth();
            if let Some(v) = verb.filter(|v| !t.bare[*v]) {
                match per_verb.iter_mut().find(|(i, _)| *i == v) {
                    Some((_, w)) => *w = w.max(value),
                    None => per_verb.push((v, value)),
                }
            }
            if found.as_ref().is_none_or(|m| value > m.value) {
                found = Some(Match {
                    term: term.to_string(),
                    token: token.to_string(),
                    field,
                    grade: g,
                    value,
                    verbs: Vec::new(),
                });
            }
        }
    };
    t.name.iter().for_each(|s| offer(Field::Name, s, None));
    t.tags.iter().for_each(|s| offer(Field::Tag, s, None));
    t.verb_names
        .iter()
        .for_each(|(i, s)| offer(Field::VerbName, s, Some(*i)));
    t.summary
        .iter()
        .for_each(|s| offer(Field::Summary, s, None));
    t.verb_summary
        .iter()
        .for_each(|(i, s)| offer(Field::VerbSummary, s, Some(*i)));
    t.body.iter().for_each(|s| offer(Field::Body, s, None));
    t.place.iter().for_each(|s| offer(Field::Place, s, None));
    if let Some(m) = found.as_mut() {
        m.verbs = per_verb;
    }
    found
}

/// How many of `n` terms a thing must match to be an answer: both of two, two of three to six, a third of a
/// longer query.
pub const fn required(n: usize) -> usize {
    if n <= 2 {
        n
    } else if n <= 6 {
        2
    } else {
        n.div_ceil(3)
    }
}

/// The things that answer `terms`, best first. Ties break on kind, then name.
pub fn rank(docs: &[Doc], terms: &[String]) -> Vec<Hit> {
    if terms.is_empty() {
        return Vec::new();
    }
    let all: Vec<Tokens> = docs.iter().map(tokens).collect();
    let per_doc: Vec<Vec<Option<Match>>> = all
        .iter()
        .map(|t| terms.iter().map(|term| best(term, t)).collect())
        .collect();
    let n = docs.len() as f64;
    let idf: Vec<f64> = (0 .. terms.len())
        .map(|i| {
            let df = per_doc.iter().filter(|m| m[i].is_some()).count().max(1) as f64;
            (1.0 + n / df).ln()
        })
        .collect();
    let idf_all: f64 = idf.iter().sum();
    let need = required(terms.len());
    let mut hits: Vec<Hit> = Vec::new();
    for (di, row) in per_doc.iter().enumerate() {
        let covered = row.iter().flatten().count();
        if covered < need {
            continue;
        }
        let mut score = 0.0;
        let mut matches = Vec::new();
        let mut by_verb: Vec<(usize, f64)> = Vec::new();
        for (i, m) in row.iter().enumerate() {
            if let Some(m) = m {
                score += m.value * idf[i];
                for &(v, w) in &m.verbs {
                    match by_verb.iter_mut().find(|(seen, _)| *seen == v) {
                        Some((_, sum)) => *sum += w * idf[i],
                        None => by_verb.push((v, w * idf[i])),
                    }
                }
                matches.push(m.clone());
            }
        }
        let seen: f64 = row
            .iter()
            .zip(&idf)
            .filter(|(m, _)| m.is_some())
            .map(|(_, w)| w)
            .sum();
        score *= (0.5 + 0.5 * seen / idf_all) * docs[di].kind.prior();
        // The verb that answers the most of the query, the earlier one where two are equal.
        let verb = by_verb
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|&(v, _)| v);
        hits.push(Hit {
            doc: di,
            score,
            matches,
            verb,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| docs[a.doc].kind.cmp(&docs[b.doc].kind))
            .then_with(|| docs[a.doc].name.cmp(&docs[b.doc].name))
    });
    if let Some(top) = hits.first().map(|h| h.score) {
        hits.retain(|h| h.score >= top * CUT);
    }
    hits
}

/// Whether a thing of kind `b` may be named beside one of kind `a`: the tools beside the tools and the
/// advice beside the advice, since a tool shares words with a rule's sentence by chance.
fn alike(a: Kind, b: Kind) -> bool {
    let tooling = |k: Kind| k.prior() >= 1.0;
    tooling(a) == tooling(b)
}

/// The words a thing is known by when it is compared with another: name, tags, place, summary and the
/// names of its verbs, each once. The words of a usage line a readme gave are not names.
fn bag(d: &Doc) -> Vec<String> {
    let mut k: Vec<String> = d
        .tags
        .iter()
        .chain(d.place.iter())
        .flat_map(|t| stems(t))
        .collect();
    k.extend(stems(&d.name));
    k.extend(stems(&d.summary));
    k.extend(
        d.verbs
            .iter()
            .filter(|v| !v.example)
            .flat_map(|v| stems(&v.name)),
    );
    k.sort();
    k.dedup();
    k
}

/// How near the words of two things are: the cosine of their bags, each word weighted by how few things
/// carry it.
pub struct Closeness {
    bags:  Vec<Vec<String>>,
    norms: Vec<f64>,
    n:     f64,
    df:    std::collections::HashMap<String, usize>,
}

impl Closeness {
    pub fn new(docs: &[Doc]) -> Self {
        let bags: Vec<Vec<String>> = docs.iter().map(bag).collect();
        let mut df: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for b in &bags {
            for w in b {
                *df.entry(w.clone()).or_default() += 1;
            }
        }
        let mut c = Self {
            norms: Vec::new(),
            n: docs.len() as f64,
            bags,
            df,
        };
        c.norms = c
            .bags
            .iter()
            .map(|b| b.iter().map(|w| c.weight(w).powi(2)).sum::<f64>().sqrt())
            .collect();
        c
    }

    fn weight(&self, w: &str) -> f64 {
        (1.0 + self.n / self.df.get(w).copied().unwrap_or(1) as f64).ln()
    }

    /// The cosine of the words of thing `a` and thing `b`.
    pub fn cos(&self, a: usize, b: usize) -> f64 {
        let dot: f64 = self.bags[a]
            .iter()
            .filter(|w| self.bags[b].contains(w))
            .map(|w| self.weight(w).powi(2))
            .sum();
        dot / (self.norms[a] * self.norms[b]).max(f64::EPSILON)
    }
}

/// For each of the first `of` answers, up to `each` other things that sit with it: those whose words
/// are closest to its own (see [`Closeness`]), at least [`CLOSE`] close, and never one that is itself an
/// answer.
pub fn related(docs: &[Doc], hits: &[Hit], of: usize, each: usize) -> Vec<Vec<usize>> {
    let close = Closeness::new(docs);
    let shown: Vec<usize> = hits.iter().map(|h| h.doc).collect();
    hits.iter()
        .take(of)
        .map(|h| {
            let mut scored: Vec<(usize, f64)> = (0 .. docs.len())
                .filter(|i| {
                    *i != h.doc && !shown.contains(i) && alike(docs[h.doc].kind, docs[*i].kind)
                })
                .map(|i| (i, close.cos(h.doc, i)))
                .filter(|(_, c)| *c >= CLOSE)
                .collect();
            scored.sort_by(|a, b| {
                b.1.total_cmp(&a.1)
                    .then_with(|| docs[a.0].kind.cmp(&docs[b.0].kind))
                    .then_with(|| docs[a.0].name.cmp(&docs[b.0].name))
            });
            scored.into_iter().take(each).map(|(i, _)| i).collect()
        })
        .collect()
}
