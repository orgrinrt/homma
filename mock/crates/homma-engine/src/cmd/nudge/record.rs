//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What the nudges remember of a session, and the log of what they did.
//!
//! Both sit under `homma/` in the workspace's git directory: one record per
//! session at `nudges/<session>.json`, and one log at `nudges.log`. Neither is
//! tracked. A record that cannot be read is a log line and an empty record, so
//! the call goes on as if nothing were remembered.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The record a session's id does not name, for a payload carrying none.
const NO_SESSION: &str = "no-session";

/// A call a row stopped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Stopped {
    pub at:      u64,
    pub tool:    String,
    pub text:    String,
    pub payload: serde_json::Value,
}

/// A stop answered, and the call it lets through.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Answered {
    pub at:   u64,
    pub text: String,
}

/// What one row remembers of the session.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RowState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said:     Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped:  Option<Stopped>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<Answered>,
    /// For a brief row, the digest of the text it was last said for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub briefed:  Option<String>,
}

/// What every row remembers of one session, by row name.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Record {
    #[serde(default)]
    pub rows: BTreeMap<String, RowState>,
}

/// Where the records and the log are.
#[derive(Debug, Clone)]
pub(crate) struct Store {
    dir: PathBuf,
}

/// One line of the log.
#[derive(Debug, serde::Serialize)]
pub(crate) struct Entry<'a> {
    pub at:      u64,
    pub session: &'a str,
    pub row:     &'a str,
    pub what:    &'a str,
    pub call:    &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub reason:  &'a str,
}

impl Store {
    /// The store at `dir`, which is `homma/nudges` in the workspace's git directory;
    /// the log is `homma/nudges.log` beside it.
    pub(crate) const fn at(dir: PathBuf) -> Store {
        Store {
            dir,
        }
    }

    fn record_path(&self, session: &str) -> PathBuf {
        let name: String = session
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let name = if name.is_empty() { NO_SESSION.to_string() } else { name };
        self.dir.join(format!("{name}.json"))
    }

    pub(crate) fn log_path(&self) -> PathBuf {
        self.dir.with_extension("log")
    }

    /// The session's record; an absent one is empty, and one that does not
    /// read is empty with the reason.
    pub(crate) fn load(&self, session: &str) -> (Record, Option<String>) {
        let path = self.record_path(session);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                match serde_json::from_str(&text) {
                    Ok(r) => (r, None),
                    Err(e) => (Record::default(), Some(format!("{}: {e}", path.display()))),
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Record::default(), None),
            Err(e) => (Record::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    /// Keep the session's record.
    pub(crate) fn keep(&self, session: &str, record: &Record) -> Result<(), String> {
        let path = self.record_path(session);
        let text = serde_json::to_string(record).map_err(|e| e.to_string())?;
        write_whole(&path, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Add one line to the log. A log that cannot be written loses the line.
    pub(crate) fn log(&self, entry: &Entry<'_>) {
        let Ok(line) = serde_json::to_string(entry) else {
            return;
        };
        let path = self.log_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Write `text` to `path` through a file beside it, so a reader never sees half.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}
