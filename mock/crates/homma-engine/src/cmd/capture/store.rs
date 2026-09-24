//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The two ends a capture sits between: where the harness keeps a workspace's
//! transcripts, and the store that already holds captures from them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use jiff::Timestamp;

/// Where the store sits under the workspace root unless `--into` says
/// otherwise.
pub const STORE: &str = ".data/op-responses";

/// The directory the harness names for a workspace path: every character
/// outside `[A-Za-z0-9]` turned into `-`.
pub fn escaped(workspace: &Path) -> String {
    workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The transcript to read: `session` as a path when a file is there, else as a
/// session id under `projects`; without one, the newest transcript there.
pub fn transcript(projects: &Path, session: Option<&str>) -> Result<PathBuf> {
    if let Some(s) = session {
        let as_path = Path::new(s);
        if as_path.is_file() {
            return Ok(as_path.to_path_buf());
        }
        let by_id = projects.join(format!("{s}.jsonl"));
        if by_id.is_file() {
            return Ok(by_id);
        }
        bail!(
            "no session `{s}`: neither a file there nor a transcript at {}",
            by_id.display()
        );
    }
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

/// The latest `through` any capture in `store` records for `session`.
///
/// A capture naming the session with a `through` that does not parse is
/// refused by file, since skipping it would move the watermark back and write
/// its events a second time.
pub fn watermark(store: &Path, session: &str) -> Result<Option<Timestamp>> {
    let mut latest: Option<Timestamp> = None;
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
        let at: Timestamp = through
            .parse()
            .with_context(|| format!("{}: `through` {through:?} is not an instant", p.display()))?;
        if latest.is_none_or(|l| at > l) {
            latest = Some(at);
        }
    }
    Ok(latest)
}

/// The later of the two floors, when there is one.
pub fn floor(watermark: Option<Timestamp>, since: Option<Timestamp>) -> Option<Timestamp> {
    match (watermark, since) {
        (Some(w), Some(s)) => Some(w.max(s)),
        (w, s) => w.or(s),
    }
}

/// The file-name slug of a title: lowercase letters and digits, runs of
/// anything else as one `-`, cut at a word boundary to keep a name readable.
pub fn slug(title: &str) -> Result<String> {
    const MOST: usize = 60;
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
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
