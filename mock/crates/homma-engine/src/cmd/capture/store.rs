//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The two ends a capture sits between: where the harness keeps a workspace's
//! transcripts, and the store that already holds captures from them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::transcript::Transcript;

/// The directory the harness names for a workspace path: every character
/// outside `[A-Za-z0-9]` turned into `-`. The harness escapes the real path, so
/// the caller resolves symlinks first.
pub fn escaped(workspace: &Path) -> String {
    workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Which session a run reads, in the order the design gives: the one named,
/// the one the run is inside, the newest.
#[derive(Debug, Clone, Copy)]
pub struct Which<'a> {
    /// `--session`, an id or a path.
    pub named:   Option<&'a str>,
    /// The session the command runs inside, as the harness names it.
    pub running: Option<&'a str>,
}

/// The transcript to read.
///
/// `projects` is the harness's directory of project directories and `own` the
/// name of the workspace's among them. A named session is a path when a file is
/// there, else an id; an id, named or running, is looked for in `own` first and
/// then in every other project directory, and is refused when found nowhere.
/// With neither, the newest transcript in `own`.
pub fn transcript(projects: &Path, own: &str, which: Which<'_>) -> Result<PathBuf> {
    if let Some(s) = which.named {
        let as_path = Path::new(s);
        if as_path.is_file() {
            return Ok(as_path.to_path_buf());
        }
    }
    if let Some(id) = which.named.or(which.running) {
        return by_id(projects, own, id);
    }
    newest(&projects.join(own))
}

/// The transcript named `id`, in `own` or else in any project directory.
fn by_id(projects: &Path, own: &str, id: &str) -> Result<PathBuf> {
    let file = format!("{id}.jsonl");
    let first = projects.join(own).join(&file);
    if first.is_file() {
        return Ok(first);
    }
    if let Ok(dirs) = std::fs::read_dir(projects) {
        let mut found: Vec<PathBuf> = dirs
            .filter_map(|d| d.ok())
            .map(|d| d.path().join(&file))
            .filter(|p| p.is_file())
            .collect();
        found.sort();
        if let Some(p) = found.into_iter().next() {
            return Ok(p);
        }
    }
    bail!(
        "no session `{id}`: neither a file there nor a transcript named for it under {}",
        projects.display()
    )
}

/// The newest transcript in one project directory.
fn newest(projects: &Path) -> Result<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let dir = std::fs::read_dir(projects).with_context(|| {
        format!(
            "no transcripts for this workspace at {}; name one with --session",
            projects.display()
        )
    })?;
    for e in dir {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        let at = std::fs::metadata(&p)?.modified()?;
        if newest.as_ref().is_none_or(|(t, _)| at > *t) {
            newest = Some((at, p));
        }
    }
    newest.map(|(_, p)| p).ok_or_else(|| {
        anyhow!(
            "no transcript in {}; name one with --session",
            projects.display()
        )
    })
}

/// The session id a transcript is named for.
pub fn session_of(transcript: &Path) -> Result<String> {
    transcript
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!(
                "{} has no name to read a session from",
                transcript.display()
            )
        })
}

/// The front matter's `key: value` lines, for the two keys the watermark reads.
fn front(text: &str) -> Option<(Option<&str>, Option<&str>)> {
    let body = text.strip_prefix("---\n")?;
    let end = body.find("\n---")?;
    let mut source = None;
    let mut through = None;
    for line in body[.. end].lines() {
        if let Some(v) = line.strip_prefix("source: ") {
            source = Some(v.trim());
        } else if let Some(v) = line.strip_prefix("through: ") {
            through = Some(v.trim());
        }
    }
    Some((source, through))
}

/// Where the last capture of a session ended: the line its `through` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub line: usize,
    pub uuid: String,
}

/// The `through` of the capture in `store` that reaches furthest into the
/// session's transcript.
///
/// Furthest in the file, not latest in time, since the file is the order the
/// harness read things in. A capture naming the session with no `through`, or
/// one the transcript does not hold, is refused by file: skipping it would
/// move the watermark back and write its events a second time.
pub fn watermark(store: &Path, session: &str, read: &Transcript) -> Result<Option<Mark>> {
    let mut latest: Option<Mark> = None;
    let entries = match std::fs::read_dir(store) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", store.display())),
    };
    for e in entries {
        let p = e?.path();
        if p.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let text =
            std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        let Some((Some(source), through)) = front(&text) else {
            continue;
        };
        if source != session {
            continue;
        }
        let through =
            through.ok_or_else(|| anyhow!("{} names the session and no `through`", p.display()))?;
        let line = read.line_of(through).ok_or_else(|| {
            anyhow!(
                "{}: `through` {through:?} is no line of the session's transcript",
                p.display()
            )
        })?;
        if latest.as_ref().is_none_or(|l| line > l.line) {
            latest = Some(Mark {
                line,
                uuid: through.to_string(),
            });
        }
    }
    Ok(latest)
}

/// The file-name slug of a title: lowercase letters and digits, `ä` and `å`
/// folded to `a` and `ö` to `o`, runs of anything else as one `-`, cut at a
/// word boundary to keep a name readable.
pub fn slug(title: &str) -> Result<String> {
    const MOST: usize = 60;
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'ä' | 'å' => 'a',
            'ö' => 'o',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > MOST {
        let cut = out[.. MOST].rfind('-').unwrap_or(MOST);
        out.truncate(cut);
    }
    if out.is_empty() {
        bail!("the title {title:?} has no letter or digit to name a file by");
    }
    Ok(out)
}

/// Every path the words mention that exists under `root`, as written relative
/// to it, in the order met and without repeats.
pub fn mentioned(root: &Path, words: &[&str]) -> Vec<String> {
    const OPENS: &[char] = &['(', '[', '{', '<', '"', '\'', '`'];
    // A dot ends a sentence and starts a hidden directory, so it is trimmed off
    // the end of a word and never off the start.
    const CLOSES: &[char] = &[')', ']', '}', '>', ',', '.', ';', ':', '"', '\'', '`', '!', '?'];
    let mut out: Vec<String> = Vec::new();
    for w in words {
        for token in w.split_whitespace() {
            let t = token.trim_start_matches(OPENS).trim_end_matches(CLOSES);
            if !t.contains('/') && !t.contains('.') {
                continue;
            }
            let p = Path::new(t);
            let rel = if p.is_absolute() {
                match p.strip_prefix(root) {
                    Ok(r) => r.to_path_buf(),
                    Err(_) => continue,
                }
            } else {
                p.to_path_buf()
            };
            let rel = rel.to_string_lossy().trim_end_matches('/').to_string();
            if rel.is_empty() || rel.split('/').any(|c| c == "..") {
                continue;
            }
            if root.join(&rel).exists() && !out.contains(&rel) {
                out.push(rel);
            }
        }
    }
    out
}
