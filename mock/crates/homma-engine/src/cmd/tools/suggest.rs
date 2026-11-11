//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The name a mistyped word most likely meant, among the built-in commands and the tools.
//!
//! A name is close when it and the word share a prefix of three letters or more (one is the start of the
//! other), or when the edit distance between them is at most a third of the word's length and never less
//! than one. Of the close names the strictly closest is the suggestion, a prefix being the closest there
//! is, and where several are equally close there is none, since a guess between them is no help.

use std::collections::BTreeSet;

/// The shortest prefix that counts as a shared start.
const MIN_PREFIX: usize = 3;

/// How far a word is from a name, lower being closer, or `None` when the name is not close.
fn distance(word: &str, name: &str) -> Option<usize> {
    let (w, n) = (word.to_lowercase(), name.to_lowercase());
    if w == n {
        return None;
    }
    if w.chars().count().min(n.chars().count()) >= MIN_PREFIX
        && (n.starts_with(&w) || w.starts_with(&n))
    {
        return Some(0);
    }
    let limit = (w.chars().count() / 3).max(1);
    Some(edits(&w, &n)).filter(|d| *d <= limit)
}

/// The edit distance of two words: insertions, deletions and substitutions.
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0 ..= b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diag + usize::from(ca != *cb)).min(up + 1).min(row[j] + 1);
            diag = up;
        }
    }
    row[b.len()]
}

/// The one name of `names` closest to `word`, when there is exactly one closest and it is close.
pub fn nearest<'a>(word: &str, names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let names: BTreeSet<&str> = names.into_iter().collect();
    let scored: Vec<(usize, &str)> = names
        .into_iter()
        .filter_map(|n| distance(word, n).map(|d| (d, n)))
        .collect();
    let best = scored.iter().map(|(d, _)| *d).min()?;
    let mut closest = scored.into_iter().filter(|(d, _)| *d == best);
    let (_, name) = closest.next()?;
    closest.next().is_none().then_some(name)
}
