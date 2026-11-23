//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A line of releases read as a sequence: where it skips a number, what
//! closes a skip, and where it first moves backwards. Nothing here reads git
//! or a registry; a step is legal as `Version::is_one_step` reads it, the one
//! definition the check, the version timeline and the estate share.

use homma_api::Version;

/// The real releases among `versions`, ordered; a pre-release is no step in
/// the line.
pub fn releases<'a>(versions: impl IntoIterator<Item = &'a Version>) -> Vec<Version> {
    let mut out: Vec<Version> = versions
        .into_iter()
        .filter(|v| v.prerelease.is_none())
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Consecutive release pairs that skip a number, in version order whatever
/// order they came in.
pub fn gaps<'a>(versions: impl IntoIterator<Item = &'a Version>) -> Vec<(Version, Version)> {
    let rel = releases(versions);
    rel.windows(2)
        .filter(|p| !p[0].is_one_step(&p[1]))
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect()
}

/// The releases out of `available` that close the gap from `low` to `high`:
/// an empty list where there is no gap, and `None` where nothing available
/// closes it, which is the opposite answer and never conflated with it.
pub fn bridge(low: &Version, high: &Version, available: &[Version]) -> Option<Vec<Version>> {
    if low.is_one_step(high) {
        return Some(Vec::new());
    }
    let mut have = releases(available);
    have.push(high.clone());
    let mut path = Vec::new();
    let mut cur = low.clone();
    while cur != *high {
        let step = have
            .iter()
            .filter(|c| *c <= high && cur.is_one_step(c) && c.prerelease.is_none())
            .min()?
            .clone();
        path.push(step.clone());
        cur = step;
    }
    path.pop();
    Some(path)
}

/// The index of the first version lower than one before it, read in the
/// order given; everything from there on sits on a line already wrong.
pub fn first_descent(ordered: &[Version]) -> Option<usize> {
    let mut high: Option<&Version> = None;
    for (i, v) in ordered.iter().enumerate() {
        if high.is_some_and(|h| v < h) {
            return Some(i);
        }
        if high.is_none_or(|h| v > h) {
            high = Some(v);
        }
    }
    None
}

/// The tagged versions a registry has skipped: not among `taken`, every
/// version it holds yanked ones included, and below the highest of them. A
/// tag above everything taken is the next release, not a hole, and nothing
/// taken at all means nothing is skipped yet.
pub fn sandwiched(tagged: &[Version], taken: &[Version]) -> Vec<Version> {
    let Some(top) = taken.iter().max() else {
        return Vec::new();
    };
    let mut out: Vec<Version> = tagged
        .iter()
        .filter(|v| !taken.contains(v) && *v < top)
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
#[path = "sequence_tests.rs"]
mod tests;
