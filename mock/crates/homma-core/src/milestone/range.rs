//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The range reports: the partitions a range splits into, and the stand-ins
//! it added.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use super::{MilestoneError, commit, contains, git};

/// The unit of a path that sits under no `crates/` directory.
pub const ROOT_UNIT: &str = "(root)";

/// The markers a stand-in is read by, in the order a line is tried against
/// them. The tool's own vocabulary, fixed here and not a content kind. The
/// second field is whether the marker counts only inside a comment, compared
/// without regard to case.
pub const MARKERS: [(&str, bool); 7] = [
    ("FIXME", false),
    ("TODO", false),
    ("todo!(", false),
    ("unimplemented!(", false),
    ("#[ignore", false),
    ("stand-in", true),
    ("relaxed", true),
];

/// The memory ordering, which says nothing about a relaxed assertion.
const ORDERING: &str = "ordering::relaxed";

/// Whether a changed file is code or prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PartitionKind {
    Source,
    Document,
}

impl PartitionKind {
    pub fn word(self) -> &'static str {
        match self {
            PartitionKind::Source => "source",
            PartitionKind::Document => "document",
        }
    }
}

/// One reviewable share of a range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Partition {
    pub unit:    String,
    pub kind:    PartitionKind,
    pub files:   u64,
    pub added:   u64,
    pub deleted: u64,
}

impl Partition {
    pub fn changed(&self) -> u64 {
        self.added + self.deleted
    }
}

/// A marker on a line the range added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StandIn {
    pub file:   String,
    pub line:   u64,
    pub marker: String,
    pub text:   String,
}

/// The crate a path belongs to: the directory after a `crates/` segment.
pub fn unit_of(path: &str) -> &str {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.iter().position(|p| *p == "crates") {
        Some(i) if i + 2 < parts.len() => parts[i + 1],
        _ => ROOT_UNIT,
    }
}

/// Whether a path is prose: markdown, a markdown template, or under `docs/`.
pub fn kind_of(path: &str) -> PartitionKind {
    if path.ends_with(".md") || path.ends_with(".md.tmpl") || path.starts_with("docs/") {
        PartitionKind::Document
    } else {
        PartitionKind::Source
    }
}

/// Resolve both ends and refuse a base the head does not contain.
fn resolve(root: &Path, base: &str, head: &str) -> Result<(String, String), MilestoneError> {
    let b = commit(root, base)?;
    let h = commit(root, head)?;
    if !contains(root, &h, &b)? {
        return Err(MilestoneError::NotAncestor {
            base: base.to_string(),
            head: head.to_string(),
        });
    }
    Ok((b, h))
}

/// The files changed between two commits with their added and deleted lines.
pub(crate) fn numstat(
    root: &Path,
    spec: &[&str],
) -> Result<Vec<(String, u64, u64)>, MilestoneError> {
    let mut args = vec!["diff", "--numstat", "-z", "--no-renames", "--no-ext-diff"];
    args.extend_from_slice(spec);
    let raw = git(root, &args)?;
    let mut out = Vec::new();
    for rec in raw.split('\0').filter(|r| !r.is_empty()) {
        let mut f = rec.splitn(3, '\t');
        let (a, d, path) = (f.next(), f.next(), f.next());
        if let (Some(a), Some(d), Some(path)) = (a, d, path) {
            // a binary file reports `-` for both
            out.push((
                path.to_string(),
                a.parse().unwrap_or(0),
                d.parse().unwrap_or(0),
            ));
        }
    }
    Ok(out)
}

/// The range split by crate and kind, the most changed first.
pub fn partition(root: &Path, base: &str, head: &str) -> Result<Vec<Partition>, MilestoneError> {
    let (b, h) = resolve(root, base, head)?;
    let mut by: BTreeMap<(String, PartitionKind), Partition> = BTreeMap::new();
    for (path, added, deleted) in numstat(root, &[&b, &h])? {
        let key = (unit_of(&path).to_string(), kind_of(&path));
        let p = by.entry(key.clone()).or_insert_with(|| {
            Partition {
                unit:    key.0.clone(),
                kind:    key.1,
                files:   0,
                added:   0,
                deleted: 0,
            }
        });
        p.files += 1;
        p.added += added;
        p.deleted += deleted;
    }
    let mut parts: Vec<Partition> = by.into_values().collect();
    parts.sort_by(|x, y| y.changed().cmp(&x.changed()));
    Ok(parts)
}

/// The text of a line after its comment opener: `//`, `/*`, `#` and a space, or
/// `<!--`, or the whole of a block comment's continuation line.
fn comment_of(line: &str) -> Option<&str> {
    if line.trim_start().starts_with('*') {
        return Some(line);
    }
    ["//", "/*", "# ", "<!--"]
        .iter()
        .filter_map(|o| line.find(o).map(|i| &line[i + o.len() ..]))
        .max_by_key(|rest| rest.len())
}

/// The first marker a line carries, by the order of [`MARKERS`].
pub fn marker_of(line: &str) -> Option<&'static str> {
    let comment = comment_of(line).map(|c| c.to_lowercase().replace(ORDERING, ""));
    MARKERS.iter().find_map(|(m, in_comment)| {
        let hit = if *in_comment {
            comment
                .as_deref()
                .is_some_and(|c| c.contains(&m.to_lowercase()))
        } else {
            line.contains(m)
        };
        hit.then_some(*m)
    })
}

/// Every marker on a line the range added, by file and line in the head.
pub fn stand_ins(root: &Path, base: &str, head: &str) -> Result<Vec<StandIn>, MilestoneError> {
    let (b, h) = resolve(root, base, head)?;
    let diff = git(root, &[
        "-c",
        "core.quotepath=off",
        "diff",
        "-U0",
        "--no-renames",
        "--no-color",
        "--no-ext-diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        &b,
        &h,
    ])?;
    let mut found = Vec::new();
    let mut file: Option<String> = None;
    let mut line = 0u64;
    let mut in_hunk = false;
    for l in diff.lines() {
        if l.starts_with("diff --git ") {
            in_hunk = false;
            file = None;
        } else if !in_hunk && l.starts_with("+++ ") {
            file = l[4 ..].strip_prefix("b/").map(|s| s.trim_end().to_string());
        } else if let Some(rest) = l.strip_prefix("@@ ") {
            in_hunk = true;
            line = hunk_start(rest);
        } else if in_hunk && l.starts_with('+') {
            let text = &l[1 ..];
            let source = file
                .as_deref()
                .is_some_and(|f| kind_of(f) == PartitionKind::Source);
            if let (Some(f), Some(m), true) = (&file, marker_of(text), source) {
                found.push(StandIn {
                    file: f.clone(),
                    line,
                    marker: m.to_string(),
                    text: text.trim().to_string(),
                });
            }
            line += 1;
        }
    }
    found.sort_by(|x, y| (&x.file, x.line).cmp(&(&y.file, y.line)));
    Ok(found)
}

/// The first new-file line of a hunk header `-a,b +c,d @@`.
fn hunk_start(header: &str) -> u64 {
    header
        .split_whitespace()
        .find_map(|t| t.strip_prefix('+'))
        .and_then(|t| t.split(',').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}
