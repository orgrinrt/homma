//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma.local.toml`: what one clone of the workspace is for.
//!
//! The manifest is committed and shared by every clone of the content
//! repository, and the same content repository is cloned once per body of
//! work, so which body of work a clone is for cannot live in it. This file sits
//! beside the manifest, is never committed, and carries two tables:
//! `[instance]`, which homma types, and `[tools.<name>]`, which it carries for
//! the workspace's own tools without reading.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file's name, beside the manifest.
pub const LOCAL_FILE: &str = "homma.local.toml";

/// The line `init` makes sure the workspace's `.gitignore` carries.
const IGNORE_LINE: &str = "/homma.local.toml";

/// Parsed `homma.local.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Local {
    pub instance: Instance,
    /// One table per workspace tool, untyped: homma holds a tool's settings
    /// without growing an opinion about them, the way `[[status.inject]]` runs
    /// a tool without knowing it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools:    BTreeMap<String, toml::Table>,
}

/// `[instance]`: what this clone is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    /// The body of work, the one key required.
    pub work:  String,
    /// What to call it where a name is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Which members the work spans. Empty means all of them.
    #[serde(default)]
    pub repos: Vec<String>,
    /// The document carrying the work's current state, from the workspace
    /// root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PathBuf>,
    /// The document carrying what the work is trying to reach, from the
    /// workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal:  Option<PathBuf>,
}

impl Local {
    /// Parse the file's text.
    pub fn parse(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    /// Read the file beside the manifest in `dir`.
    ///
    /// `Ok(None)` where there is no file, which is the ordinary case. A file
    /// that is there and does not parse is an error naming it: a setting
    /// silently ignored is worse than a command that refuses.
    pub fn load(dir: &Path) -> Result<Option<Self>, LocalError> {
        let path = dir.join(LOCAL_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(LocalError::Io {
                    path,
                    source,
                });
            },
        };
        Self::parse(&text).map(Some).map_err(|source| {
            LocalError::Parse {
                path,
                source,
            }
        })
    }
}

/// The text `homma local init` writes: every key, the optional ones commented
/// until somebody fills them.
pub fn skeleton(work: &str) -> String {
    let work = toml_edit::Value::from(work).to_string();
    format!(
        "# What this clone of the workspace is for. Never committed.\n\
         [instance]\n\
         work = {work}\n\
         # title = \"\"\n\
         repos = []\n\
         # state = \"\"\n\
         # goal = \"\"\n\
         \n\
         # The workspace's own tools keep their settings here, one table each,\n\
         # as [tools.<name>].\n"
    )
}

/// Write one string at `key` into the file's text and return the new text.
///
/// `key` is `instance.<key>` or `tools.<name>.<key>`, and nothing else is
/// writable: the file's other tables are nobody's. Comments and layout around
/// the value are kept, and a result that would not load is refused, so a
/// string written where the schema wants a list never reaches the disk.
pub fn set(text: &str, key: &str, value: &str) -> Result<String, LocalError> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(LocalError::Key(key.to_owned()));
    }
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(LocalError::Edit)?;
    match parts.as_slice() {
        ["instance", field] => {
            let instance = doc
                .entry("instance")
                .or_insert(toml_edit::table())
                .as_table_mut()
                .ok_or_else(|| LocalError::Key(key.to_owned()))?;
            instance[*field] = toml_edit::value(value);
        },
        ["tools", tool, field] => {
            let tools = doc.entry("tools").or_insert_with(|| {
                let mut t = toml_edit::Table::new();
                // No bare `[tools]` header above the first tool's table.
                t.set_implicit(true);
                toml_edit::Item::Table(t)
            });
            let tool = tools
                .as_table_mut()
                .ok_or_else(|| LocalError::Key(key.to_owned()))?
                .entry(tool)
                .or_insert(toml_edit::table())
                .as_table_mut()
                .ok_or_else(|| LocalError::Key(key.to_owned()))?;
            tool[*field] = toml_edit::value(value);
        },
        _ => return Err(LocalError::Key(key.to_owned())),
    }
    let out = doc.to_string();
    Local::parse(&out).map_err(LocalError::Refused)?;
    Ok(out)
}

/// Make sure the workspace's `.gitignore` in `root` names the file, appending
/// the line where none does. `true` when it wrote.
pub fn ensure_ignored(root: &Path) -> std::io::Result<bool> {
    let path = root.join(".gitignore");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if text
        .lines()
        .any(|l| matches!(l.trim(), IGNORE_LINE | LOCAL_FILE))
    {
        return Ok(false);
    }
    let mut out = text;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(IGNORE_LINE);
    out.push('\n');
    std::fs::write(&path, out)?;
    Ok(true)
}

/// What reading or writing the file can fail on.
#[derive(Debug)]
pub enum LocalError {
    /// The file is there and does not parse.
    Parse {
        path:   PathBuf,
        source: toml::de::Error,
    },
    /// The file could not be read.
    Io {
        path:   PathBuf,
        source: std::io::Error,
    },
    /// A key outside `instance.<key>` and `tools.<name>.<key>`.
    Key(String),
    /// The text handed to `set` is not TOML at all.
    Edit(toml_edit::TomlError),
    /// The value would leave a file that does not load.
    Refused(toml::de::Error),
}

impl std::fmt::Display for LocalError {
    /// What went wrong, and not what the thing under it said; the source
    /// carries that, and printing both renders it twice.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse {
                path,
                ..
            } => write!(f, "{} does not parse", path.display()),
            Self::Io {
                path,
                ..
            } => write!(f, "failed to read {}", path.display()),
            Self::Key(key) => {
                write!(
                    f,
                    "`{key}` is not writable: only `instance.<key>` and `tools.<name>.<key>` are"
                )
            },
            Self::Edit(_) => write!(f, "{LOCAL_FILE} is not TOML"),
            Self::Refused(_) => write!(f, "that value would leave {LOCAL_FILE} unable to load"),
        }
    }
}

impl std::error::Error for LocalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse {
                source,
                ..
            } => Some(source),
            Self::Io {
                source,
                ..
            } => Some(source),
            Self::Key(_) => None,
            Self::Edit(e) => Some(e),
            Self::Refused(e) => Some(e),
        }
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
