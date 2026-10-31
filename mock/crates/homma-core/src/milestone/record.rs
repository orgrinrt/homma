//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The record: `.homma/milestones.toml`, one table per milestone, in order,
//! and the milestone tags that answer a repository without one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{MilestoneError, commit, contains, exists, git, trunk_here};

/// Where the record sits, relative to the repository's root.
pub const RECORD_PATH: &str = ".homma/milestones.toml";

/// The prefix of a milestone tag.
pub const TAG_PREFIX: &str = "milestone/";

/// One milestone as recorded. Every field but the revisions it was made of is
/// optional because a row is written in steps, and a row from before the steps
/// holds `sha` and `at` only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Its number; a row without one is numbered by its place in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n:         Option<usize>,
    /// The previous milestone's tip, absent where none is recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base:      Option<String>,
    /// The trunk's tip when the freeze began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze:    Option<String>,
    /// The committer date of `freeze`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_at: Option<String>,
    /// The revision the review pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head:      Option<String>,
    /// The trunk's tip at close, the next milestone's base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip:       Option<String>,
    /// A row from before the steps: its `tip`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha:       Option<String>,
    /// The committer date of the tip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at:        Option<String>,
}

impl Row {
    /// Set the key named `key` to `value`; false where a row has no such key.
    fn put(&mut self, key: &str, value: &str) -> bool {
        let slot = match key {
            "base" => &mut self.base,
            "freeze" => &mut self.freeze,
            "freeze_at" => &mut self.freeze_at,
            "head" => &mut self.head,
            "tip" => &mut self.tip,
            "at" => &mut self.at,
            _ => return false,
        };
        *slot = Some(value.to_string());
        true
    }

    /// Where the milestone closed: `tip`, or `sha` on a row of the old shape.
    pub fn tip(&self) -> Option<&str> {
        self.tip.as_deref().or(self.sha.as_deref())
    }

    /// The row as the lines of a new table, in the order of the design, each key it
    /// holds and no other.
    fn lines(&self, eol: &str) -> String {
        let text = [
            ("base", &self.base),
            ("freeze", &self.freeze),
            ("freeze_at", &self.freeze_at),
            ("head", &self.head),
            ("tip", &self.tip),
            ("sha", &self.sha),
            ("at", &self.at),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_ref().map(|v| format!("{key} = \"{v}\"{eol}")));
        self.n
            .map(|n| format!("n = {n}{eol}"))
            .into_iter()
            .chain(text)
            .collect()
    }
}

/// A close made: the tip and the date of its commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub sha: String,
    pub at:  String,
}

/// Where `base` found its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Record,
    Tag,
}

#[derive(Deserialize, Default)]
struct File {
    #[serde(default)]
    milestone: Vec<Row>,
}

/// The number of the row at `index`: its own, or its place in the file.
pub fn number(rows: &[Row], index: usize) -> usize {
    rows[index].n.unwrap_or(index + 1)
}

/// The rows in the working tree's record, oldest first; none where there is
/// no file. The steps write to this one.
pub fn rows(root: &Path) -> Result<Vec<Row>, MilestoneError> {
    let path = root.join(RECORD_PATH);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(MilestoneError::Record(e.to_string())),
    };
    parse(&text)
}

fn parse(text: &str) -> Result<Vec<Row>, MilestoneError> {
    let file: File = toml::from_str(text).map_err(|e| MilestoneError::Record(e.to_string()))?;
    Ok(file.milestone)
}

/// The file's own line ending: CRLF where its first line ends in one, LF
/// otherwise.
fn eol(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if text[.. i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// The body of each `[[milestone]]` table in `text`, as the byte range from the
/// line after its header to the next table header or the end. A leading byte
/// order mark does not hide the first header.
fn tables(text: &str) -> Vec<(usize, usize)> {
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let end = at + line.len();
        let head = line.trim_start_matches('\u{feff}').trim();
        let name: String = head
            .split('#')
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if head.starts_with('[') {
            if let Some(last) = found.last_mut() {
                if last.1 == usize::MAX {
                    last.1 = at;
                }
            }
            if name == "[[milestone]]" {
                found.push((end, usize::MAX));
            }
        }
        at = end;
    }
    if let Some(last) = found.last_mut() {
        if last.1 == usize::MAX {
            last.1 = text.len();
        }
    }
    found
}

/// The record's text with the tables a step may edit, refused where the rows it
/// parses as are not exactly those tables (an inline array, say).
fn editable(text: &str) -> Result<(Vec<(usize, usize)>, Vec<Row>), MilestoneError> {
    let spans = tables(text);
    let rows = parse(text)?;
    if rows.len() == spans.len() {
        Ok((spans, rows))
    } else {
        Err(MilestoneError::Record(
            "the record is not written as `[[milestone]]` tables, so a step cannot edit it".into(),
        ))
    }
}

/// Add `row` to the record as a new table at its end, creating the file and its
/// directory. Every byte already there stays as it was, and the lines added end
/// as the file's own do.
pub fn append(root: &Path, row: &Row) -> Result<(), MilestoneError> {
    let path = root.join(RECORD_PATH);
    let mut text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(MilestoneError::Io(e)),
    };
    let (_, before) = editable(&text)?;
    let eol = eol(&text);
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push_str(eol);
        }
        text.push_str(eol);
    }
    text.push_str("[[milestone]]");
    text.push_str(eol);
    text.push_str(&row.lines(eol));
    let mut expected = before;
    expected.push(row.clone());
    reads_back(&text, &expected)?;
    store(&path, &text)
}

/// The text of `line` with the value of `key` replaced by `value`, keeping the
/// indent, and whatever follows the old value on the line, when the line sets
/// `key`.
fn replaced(line: &str, key: &str, value: &str) -> Option<String> {
    let indent = line.len() - line.trim_start().len();
    let rest = line[indent ..]
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?;
    let rest = rest.trim_start();
    let kept = match rest.strip_prefix('\'') {
        Some(inner) => &inner[inner.find('\'')? + 1 ..],
        None => rest_after_basic(rest)?,
    };
    Some(format!("{}{key} = \"{value}\"{kept}", &line[.. indent]))
}

fn rest_after_basic(rest: &str) -> Option<&str> {
    Some(match rest.strip_prefix('"') {
        Some(inner) => {
            let mut escaped = false;
            let close = inner.char_indices().find(|&(_, c)| {
                let end = c == '"' && !escaped;
                escaped = c == '\\' && !escaped;
                end
            })?;
            &inner[close.0 + 1 ..]
        },
        None => {
            rest.find(|c: char| c.is_whitespace() || c == '#')
                .map_or("", |i| &rest[i ..])
        },
    })
}

/// Set `pairs` on the table at `index`: a key already there has its value
/// replaced where it stands, and a key not there is added after the table's last
/// key. Every other byte of the file, a comment, the order of keys, a key this
/// shape does not name, the line endings and a leading byte order mark, stays.
pub fn set(root: &Path, index: usize, pairs: &[(&str, &str)]) -> Result<(), MilestoneError> {
    let path = root.join(RECORD_PATH);
    let text = std::fs::read_to_string(&path).map_err(MilestoneError::Io)?;
    let (spans, before) = editable(&text)?;
    let &(start, end) = spans
        .get(index)
        .ok_or_else(|| MilestoneError::Record(format!("no table at index {index}")))?;
    let eol = eol(&text);
    let mut body = text[start .. end].to_string();
    for (key, value) in pairs {
        let mut at = 0;
        let mut after_last_key = 0;
        let mut done = false;
        for line in body.clone().split_inclusive('\n') {
            if let Some(new) = replaced(line, key, value) {
                body.replace_range(at .. at + line.len(), &new);
                done = true;
                break;
            }
            at += line.len();
            let code = line.trim_start();
            if code.contains('=') && !code.starts_with('#') {
                after_last_key = at;
            }
        }
        if !done {
            let mut add = format!("{key} = \"{value}\"{eol}");
            if after_last_key > 0 && !body[.. after_last_key].ends_with('\n') {
                add.insert_str(0, eol);
            }
            body.insert_str(after_last_key, &add);
        }
    }
    let mut out = text[.. start].to_string();
    // a header on the last line of a file with no line ending takes one first
    if start == text.len() && !out.ends_with('\n') {
        out.push_str(eol);
    }
    out.push_str(&body);
    out.push_str(&text[end ..]);
    let mut expected = before;
    for (key, value) in pairs {
        if !expected[index].put(key, value) {
            return Err(MilestoneError::Record(format!(
                "`{key}` is no key of a milestone row"
            )));
        }
    }
    reads_back(&out, &expected)?;
    store(&path, &out)
}

/// Refuse text that does not read back as exactly `expected`: a step writes by
/// position, so what it wrote is checked by parsing it, which is what keeps a
/// multi-line value or a quoting it did not expect from being broken in silence.
fn reads_back(text: &str, expected: &[Row]) -> Result<(), MilestoneError> {
    if parse(text)? == expected {
        Ok(())
    } else {
        Err(MilestoneError::Record(
            "the step would not read back as the row it sets, so the record is left as it is"
                .into(),
        ))
    }
}

fn store(path: &Path, text: &str) -> Result<(), MilestoneError> {
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, text))
        .map_err(MilestoneError::Io)
}

/// The rows in the record as the trunk commits it, oldest first; none where
/// the trunk has no file. A clone checked out on a branch answers as the trunk.
pub fn trunk_rows(root: &Path, trunk_ref: &str) -> Result<Vec<Row>, MilestoneError> {
    let spec = format!("{trunk_ref}:{RECORD_PATH}");
    let listing = git(root, &[
        "ls-tree",
        "--name-only",
        trunk_ref,
        "--",
        RECORD_PATH,
    ])?;
    if listing.trim().is_empty() {
        return Ok(Vec::new());
    }
    parse(&git(root, &["show", &spec])?)
}

/// The newest `milestone/*` tag the trunk contains, as the commit it names.
pub(super) fn newest_tag(root: &Path, trunk_ref: &str) -> Result<Option<String>, MilestoneError> {
    let listing = git(root, &[
        "tag",
        "--list",
        "milestone/*",
        "--merged",
        trunk_ref,
        "--sort=-version:refname",
    ])?;
    listing
        .lines()
        .next()
        .map(|tag| commit(root, tag))
        .transpose()
}

/// Where the current milestone began: the last tip in the trunk's committed
/// record, or the newest `milestone/*` tag the trunk contains where the record
/// has none. While a milestone is open this is the sha it began from.
pub fn base(root: &Path, trunk: &str) -> Result<(String, Source), MilestoneError> {
    let trunk_ref = trunk_here(root, trunk);
    let tip = trunk_rows(root, &trunk_ref)?
        .iter()
        .rev()
        .find_map(|r| r.tip().map(str::to_string));
    if let Some(sha) = tip {
        return Ok((sha, Source::Record));
    }
    match newest_tag(root, &trunk_ref)? {
        Some(sha) => Ok((sha, Source::Tag)),
        None => Err(MilestoneError::NoMilestone),
    }
}

/// Whether the trunk, here or on origin, contains `sha`.
pub(super) fn on_trunk(root: &Path, trunk: &str, sha: &str) -> Result<bool, MilestoneError> {
    if exists(root, trunk) && contains(root, trunk, sha)? {
        return Ok(true);
    }
    let remote = format!("origin/{trunk}");
    Ok(exists(root, &remote) && contains(root, &remote, sha)?)
}
