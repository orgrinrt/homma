//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma capture --title <title> [--session <id|path>] [--since <instant>]
//! [--bears-on <path>]... [--into <dir>]`.
//!
//! Copies what the person said to an agent session, and every question put to
//! them with its options and their answer, out of the session's transcript and
//! into one capture file in the workspace's store. The words come out of the
//! transcript's structured fields as they are; the command writes the file and
//! leaves committing it to the caller.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use homma_core::Config;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

pub mod render;
pub mod store;
pub mod transcript;

#[cfg(test)]
mod tests;

/// What a run was asked for.
pub struct Ask<'a> {
    pub title:    &'a str,
    pub session:  Option<&'a str>,
    pub since:    Option<Timestamp>,
    pub bears_on: &'a [String],
    pub into:     Option<&'a Path>,
}

/// What a run did.
#[derive(Debug, Serialize)]
pub struct CaptureReport {
    pub session: String,
    /// The file written, or none when the session held nothing new.
    pub written: Option<PathBuf>,
    pub said:    usize,
    pub asked:   usize,
    /// Where the run started from, when anything set a floor.
    pub after:   Option<String>,
}

impl HumanRender for CaptureReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        match &self.written {
            Some(p) => {
                write!(
                    out,
                    "wrote {} ({} said, {} asked)",
                    p.display(),
                    self.said,
                    self.asked
                )
            },
            None => {
                match &self.after {
                    Some(a) => write!(out, "nothing in {} after {a}", self.session),
                    None => write!(out, "nothing in {}", self.session),
                }
            },
        }
    }
}

/// A capture made and not yet written.
#[derive(Debug)]
pub struct Made {
    pub name:  String,
    pub body:  String,
    pub said:  usize,
    pub asked: usize,
}

/// Where the harness keeps this workspace's transcripts.
fn projects(root: &Path) -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow!("no HOME to find the transcripts under; name one with --session"))?;
    Ok(PathBuf::from(home)
        .join(".claude/projects")
        .join(store::escaped(root)))
}

/// The capture a run makes, before anything touches the disk.
///
/// Split from [`run`] so the whole path from transcript to file text is
/// tested without a home directory or a clock.
pub fn make(
    root: &Path,
    text: &str,
    session: &str,
    floor: Option<Timestamp>,
    ask: &Ask<'_>,
    zone: &TimeZone,
) -> Result<Option<Made>> {
    let slug = store::slug(ask.title)?;
    let events: Vec<_> = transcript::read(text)?
        .into_iter()
        .filter(|e| floor.is_none_or(|f| e.at > f))
        .collect();
    let Some(first) = events.first() else {
        return Ok(None);
    };
    let mut bears_on: Vec<String> = ask.bears_on.to_vec();
    let words: Vec<&str> = events.iter().flat_map(|e| e.words()).collect();
    for p in store::mentioned(root, &words) {
        if !bears_on.contains(&p) {
            bears_on.push(p);
        }
    }
    let name = render::file_name(first.at, zone, &slug);
    let body = render::render(&render::Capture {
        title: ask.title,
        session,
        events: &events,
        bears_on: &bears_on,
        zone,
    });
    let said = events
        .iter()
        .filter(|e| matches!(e.what, transcript::Happened::Said(_)))
        .count();
    Ok(Some(Made {
        name,
        body,
        said,
        asked: events.len() - said,
    }))
}

pub fn run(cfg: &Config, ask: &Ask<'_>, format: OutputFormat) -> Result<()> {
    let root = cfg.workspace.path.as_path();
    let path = store::transcript(&projects(root)?, ask.session)?;
    let session = store::session_of(&path)?;
    let dir = ask
        .into
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(store::STORE));
    let floor = store::floor(store::watermark(&dir, &session)?, ask.since);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let made = make(root, &text, &session, floor, ask, &TimeZone::system())?;
    let after = floor.map(|f| f.to_string());
    let report = match made {
        None => {
            CaptureReport {
                session,
                written: None,
                said: 0,
                asked: 0,
                after,
            }
        },
        Some(Made {
            name,
            body,
            said,
            asked,
        }) => {
            std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
            let file = dir.join(name);
            if file.exists() {
                bail!(
                    "{} is already there, and a capture is never written over",
                    file.display()
                );
            }
            std::fs::write(&file, body).with_context(|| format!("writing {}", file.display()))?;
            CaptureReport {
                session,
                written: Some(file),
                said,
                asked,
                after,
            }
        },
    };
    emit(&report, format)?;
    Ok(())
}
