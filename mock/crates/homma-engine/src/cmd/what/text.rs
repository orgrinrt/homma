//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Words: what a query and a description are reduced to, and what counts as the same word.
//!
//! The text is lowercased and split at everything that is not a letter or a digit. A word is reduced
//! to a stem by taking off a plural, `ing`, `ed` and a final `e`, so `landed`, `lands` and `landing`
//! are one stem. Two stems are the same word, a word of the same group, a prefix of each other or a
//! misspelling of each other, and [`grade`] says which, strongest first.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Words that carry no subject: the shape of a question rather than what it asks about.
const FILLER: &[&str] = &[
    "a",
    "an",
    "the",
    "to",
    "for",
    "of",
    "in",
    "on",
    "at",
    "by",
    "and",
    "or",
    "is",
    "are",
    "be",
    "do",
    "does",
    "did",
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "i",
    "my",
    "me",
    "we",
    "our",
    "you",
    "your",
    "use",
    "using",
    "used",
    "what",
    "which",
    "how",
    "where",
    "when",
    "who",
    "can",
    "could",
    "should",
    "would",
    "will",
    "want",
    "need",
    "tool",
    "tools",
    "command",
    "script",
    "with",
    "from",
    "into",
    "some",
    "any",
    "there",
    "something",
    "way",
    "thing",
    "get",
    "please",
    "has",
    "have",
    "had",
    "was",
    "were",
    "about",
    "up",
    "out",
    "so",
    "if",
    "than",
    "just",
    "also",
    "not",
    "no",
    "one",
    "all",
];

/// Groups of words that mean the same in this tooling. A vocabulary and not a list of tools: no tool
/// is named here, and a group is added with a test beside it.
pub(super) const SYNONYMS: &[&[&str]] = &[
    &["land", "integrate", "ship"],
    &["done", "finished", "finish", "complete", "completed"],
    &["disk", "space", "storage"],
    &[
        "free", "reclaim", "trim", "clean", "cleanup", "purge", "prune", "reap", "sweep", "gc",
    ],
    &[
        "lost",
        "stray",
        "orphan",
        "leftover",
        "abandoned",
        "dangling",
        "runaway",
        "stuck",
        "hung",
    ],
    &["end", "ended", "stop", "kill", "terminate", "halt"],
    &["message", "msg", "mail", "dm", "send", "post", "notify", "tell", "channel"],
    &["helper", "helpers", "session", "sessions", "worker", "wrapper"],
    &["open", "start", "begin", "create", "new"],
    &["check", "verify", "audit", "validate", "inspect"],
    &["branch", "branches", "ref", "refs"],
    &["delete", "remove", "drop", "discard"],
    &["find", "search", "locate", "discover", "lookup"],
    &["fetch", "download", "pull", "get"],
    &["say", "said", "answer", "answered", "reply", "respond", "ask", "asked"],
    &["rule", "rules", "policy", "convention"],
    &["test", "tests", "suite", "spec"],
    &["build", "compile", "make"],
];

/// The lowercase words of a text, a letter or a digit run each, single letters dropped.
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

/// Whether a word is filler.
pub fn is_filler(w: &str) -> bool {
    FILLER.contains(&w)
}

/// The words of a query that carry a subject, each with its stem, each stem once, in the order given.
pub fn pairs(query: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for w in words(query) {
        if is_filler(&w) {
            continue;
        }
        let s = stem(&w);
        if !out.iter().any(|(_, t)| *t == s) {
            out.push((w, s));
        }
    }
    out
}

/// The stem of a lowercase word.
pub fn stem(w: &str) -> String {
    let mut s = w.to_string();
    let n = s.chars().count();
    if n > 4 && s.ends_with("ies") {
        s.truncate(s.len() - 3);
        s.push('y');
    } else if n > 5 && s.ends_with("ing") {
        s.truncate(s.len() - 3);
        undouble(&mut s);
    } else if n > 4 && s.ends_with("ed") {
        s.truncate(s.len() - 2);
        undouble(&mut s);
    } else if n > 4
        && (s.ends_with("sses") || s.ends_with("shes") || s.ends_with("ches") || s.ends_with("xes"))
    {
        s.truncate(s.len() - 2);
    } else if n > 3 && s.ends_with('s') && !s.ends_with("ss") && !s.ends_with("us") {
        s.pop();
    }
    if s.chars().count() > 3 && s.ends_with('e') {
        s.pop();
    }
    s
}

/// `runn` is `run`: a doubled consonant the suffix brought in.
fn undouble(s: &mut String) {
    let b = s.as_bytes();
    let n = b.len();
    if n >= 4 && b[n - 1] == b[n - 2] && !b"aeiouls".contains(&b[n - 1]) {
        s.pop();
    }
}

/// The group a stem belongs to, if any.
pub fn group_of(stem_word: &str) -> Option<usize> {
    static MAP: OnceLock<HashMap<String, usize>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, g) in SYNONYMS.iter().enumerate() {
            for w in *g {
                m.entry(stem(w)).or_insert(i);
            }
        }
        m
    })
    .get(stem_word)
    .copied()
}

/// How a query stem matched a stem of the text, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Fuzzy,
    Prefix,
    Synonym,
    Same,
}

impl Grade {
    /// What the match is worth, as a share of an exact one.
    pub const fn worth(self) -> f64 {
        match self {
            Self::Same => 1.0,
            Self::Synonym => 0.75,
            Self::Prefix => 0.65,
            Self::Fuzzy => 0.55,
        }
    }

    /// The mark printed between the two words.
    pub const fn mark(self) -> &'static str {
        match self {
            Self::Same => "=",
            Self::Synonym => "~syn~",
            Self::Prefix => "~pre~",
            Self::Fuzzy => "~typo~",
        }
    }
}

/// How the query stem `q` matches the text stem `t`, or not at all.
pub fn grade(q: &str, t: &str) -> Option<Grade> {
    if q == t {
        return Some(Grade::Same);
    }
    if let (Some(a), Some(b)) = (group_of(q), group_of(t)) {
        if a == b {
            return Some(Grade::Synonym);
        }
    }
    let (ql, tl) = (q.chars().count(), t.chars().count());
    if ql >= 4 && tl > ql && t.starts_with(q) || tl >= 4 && ql > tl && q.starts_with(t) {
        return Some(Grade::Prefix);
    }
    if ql == tl && ql >= 3 && edit_distance(q, t) == 1 && swapped(q, t) {
        return Some(Grade::Fuzzy);
    }
    let allowed = match ql.min(tl) {
        0 ..= 4 => 0,
        5 ..= 7 => 1,
        _ => 2,
    };
    if allowed > 0 && ql.abs_diff(tl) <= allowed && edit_distance(q, t) <= allowed {
        return Some(Grade::Fuzzy);
    }
    None
}

/// Whether two words of one length differ by one adjacent pair swapped, which is the slip a short word
/// can take and still be itself: `dsik` for `disk`.
fn swapped(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    (1 .. a.len()).any(|i| {
        a[i - 1] == b[i]
            && a[i] == b[i - 1]
            && a[.. i - 1] == b[.. i - 1]
            && a[i + 1 ..] == b[i + 1 ..]
    })
}

/// The Damerau-Levenshtein distance with adjacent transpositions, over characters.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1 ..= n {
        for j in 1 ..= m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}
