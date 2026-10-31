//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The steps that write a milestone's row, `freeze`, `pin` and `mark`, and
//! `show`, which reads the rows with their ranges.

use std::path::Path;

use serde::Serialize;

use super::record::{Mark, Row, TAG_PREFIX, append, number, on_trunk, rows, set, trunk_rows};
use super::{MilestoneError, commit, contains, exists, git, trunk_here};

/// The trunk's tip as a freeze or a pin reads it: `origin`'s where the clone
/// has one, since a local copy may be stale, and the local branch otherwise.
fn trunk_tip(root: &Path, trunk: &str) -> Result<String, MilestoneError> {
    let remote = format!("origin/{trunk}");
    if exists(root, &remote) { commit(root, &remote) } else { commit(root, trunk) }
}

fn date(root: &Path, sha: &str) -> Result<String, MilestoneError> {
    Ok(git(root, &["show", "-s", "--format=%cs", sha])?
        .trim()
        .to_string())
}

/// The open milestone's index: the last row, where it has no tip.
fn open(rows: &[Row]) -> Option<usize> {
    rows.len()
        .checked_sub(1)
        .filter(|&i| rows[i].tip().is_none())
}

/// The number of the milestone after the last row.
fn next(rows: &[Row]) -> usize {
    rows.len().checked_sub(1).map_or(1, |i| number(rows, i) + 1)
}

/// `sha` as a commit on the trunk that contains `of`.
fn after(
    root: &Path,
    trunk: &str,
    rev: &str,
    of: &str,
    what: &'static str,
) -> Result<String, MilestoneError> {
    let sha = commit(root, rev)?;
    if !on_trunk(root, trunk, &sha)? {
        return Err(MilestoneError::NotOnTrunk {
            sha:   rev.to_string(),
            trunk: trunk.to_string(),
        });
    }
    if !contains(root, &sha, of)? {
        return Err(MilestoneError::NotDescendant {
            sha: rev.to_string(),
            of: of.to_string(),
            what,
        });
    }
    Ok(sha)
}

/// Open the next milestone: its `base` is the last tip, its `freeze` the
/// trunk's tip now. Commits nothing.
pub fn freeze(root: &Path, trunk: &str) -> Result<Row, MilestoneError> {
    let all = rows(root)?;
    if let Some(i) = open(&all) {
        return Err(MilestoneError::StillOpen(number(&all, i)));
    }
    let at = trunk_tip(root, trunk)?;
    // the first row has no base: a tag answers the `base` command and nothing else
    let base = all.last().and_then(|r| r.tip().map(str::to_string));
    if let Some(base) = &base {
        if !contains(root, &at, base)? {
            return Err(MilestoneError::NotAncestor {
                base: base.clone(),
                head: at,
            });
        }
    }
    let row = Row {
        n: Some(next(&all)),
        base,
        freeze_at: Some(date(root, &at)?),
        freeze: Some(at),
        ..Row::default()
    };
    append(root, &row)?;
    Ok(row)
}

/// Set the open milestone's `head`: `rev`, or the trunk's tip where none is
/// given. Commits nothing.
pub fn pin(root: &Path, trunk: &str, rev: Option<&str>) -> Result<Row, MilestoneError> {
    let mut all = rows(root)?;
    let i = open(&all).ok_or(MilestoneError::NotOpen)?;
    let n = number(&all, i);
    let freeze = all[i].freeze.clone().ok_or(MilestoneError::NoFreeze(n))?;
    if all[i].head.is_some() {
        return Err(MilestoneError::AlreadyPinned(n));
    }
    let head = match rev {
        Some(rev) => after(root, trunk, rev, &freeze, "freeze")?,
        None => after(root, trunk, &trunk_tip(root, trunk)?, &freeze, "freeze")?,
    };
    set(root, i, &[("head", &head)])?;
    all[i].head = Some(head);
    Ok(all[i].clone())
}

/// Close the open milestone at `rev`, setting its `tip` and `at`; with no
/// milestone open, append a row holding the tip alone. With `tag`, also make
/// the annotated tag `milestone/<n>` here. Commits nothing and pushes nothing.
pub fn mark(root: &Path, trunk: &str, rev: &str, tag: bool) -> Result<Mark, MilestoneError> {
    let sha = commit(root, rev)?;
    if !on_trunk(root, trunk, &sha)? {
        return Err(MilestoneError::NotOnTrunk {
            sha:   rev.to_string(),
            trunk: trunk.to_string(),
        });
    }
    let all = rows(root)?;
    if all.iter().any(|r| r.tip() == Some(sha.as_str())) {
        return Err(MilestoneError::AlreadyMarked(rev.to_string()));
    }
    let at = date(root, &sha)?;
    let open = open(&all);
    if let Some(i) = open {
        let head = all[i]
            .head
            .clone()
            .ok_or(MilestoneError::NotPinned(number(&all, i)))?;
        after(root, trunk, rev, &head, "head")?;
    } else if let Some(last) = all.last().and_then(|r| r.tip()) {
        after(root, trunk, rev, last, "tip")?;
    }
    let n = open.map_or_else(|| next(&all), |i| number(&all, i));
    // the tag first: a refused tag then leaves the record as it was, and a
    // rerun is not met by "already marked"
    let tagged = if tag { Some(make_tag(root, n, &sha)?) } else { None };
    let written = match open {
        Some(i) => set(root, i, &[("tip", &sha), ("at", &at)]),
        None => {
            append(root, &Row {
                n: Some(n),
                tip: Some(sha.clone()),
                at: Some(at.clone()),
                ..Row::default()
            })
        },
    };
    if let Err(e) = written {
        if let Some(name) = tagged {
            let _ = git(root, &["tag", "-d", &name]);
        }
        return Err(e);
    }
    Ok(Mark {
        sha,
        at,
    })
}

/// Make the annotated tag `milestone/<n>` at `sha`, `n` being the first
/// number from `from` up whose tag is free; its name.
fn make_tag(root: &Path, from: usize, sha: &str) -> Result<String, MilestoneError> {
    let mut n = from;
    let name = loop {
        let name = format!("{TAG_PREFIX}{n}");
        if !exists(root, &name) {
            break name;
        }
        n += 1;
    };
    git(root, &[
        "tag",
        "-a",
        &name,
        sha,
        "-m",
        &format!("milestone {n}"),
    ])?;
    Ok(name)
}

/// A recorded milestone with the two ranges its revisions give.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub n:         usize,
    pub base:      Option<String>,
    pub freeze:    Option<String>,
    pub freeze_at: Option<String>,
    pub head:      Option<String>,
    pub tip:       Option<String>,
    pub at:        Option<String>,
    /// `base..freeze`: the work the milestone closes.
    pub epoch:     Option<String>,
    /// `base..head`: what the review covers.
    pub review:    Option<String>,
}

fn range(from: &Option<String>, to: &Option<String>) -> Option<String> {
    Some(format!("{}..{}", from.as_ref()?, to.as_ref()?))
}

/// The rows as the trunk commits them, each with its ranges; row `n` alone
/// where `n` is given. A row with no base of its own takes the previous tip.
pub fn show(root: &Path, trunk: &str, n: Option<usize>) -> Result<Vec<Shown>, MilestoneError> {
    let all = trunk_rows(root, &trunk_here(root, trunk))?;
    let shown: Vec<Shown> = (0 .. all.len()).map(|i| shown_at(&all, i)).collect();
    match n {
        None => Ok(shown),
        Some(n) => {
            let one: Vec<Shown> = shown.into_iter().filter(|s| s.n == n).collect();
            if one.is_empty() { Err(MilestoneError::NoSuchMilestone(n)) } else { Ok(one) }
        },
    }
}

/// Row `i` of `all` with its ranges; a row with no base of its own takes the
/// previous tip. The one place a row becomes a `Shown`.
fn shown_at(all: &[Row], i: usize) -> Shown {
    let r = &all[i];
    let base = r.base.clone().or_else(|| {
        i.checked_sub(1)
            .and_then(|p| all[p].tip().map(str::to_string))
    });
    Shown {
        n: number(all, i),
        epoch: range(&base, &r.freeze),
        review: range(&base, &r.head),
        base,
        freeze: r.freeze.clone(),
        freeze_at: r.freeze_at.clone(),
        head: r.head.clone(),
        tip: r.tip().map(str::to_string),
        at: r.at.clone(),
    }
}

/// The newest row of the working tree's record as `show` prints it: what a
/// step has just written.
pub fn latest(root: &Path) -> Result<Option<Shown>, MilestoneError> {
    let all = rows(root)?;
    Ok(all.len().checked_sub(1).map(|i| shown_at(&all, i)))
}
