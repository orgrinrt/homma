//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading `homma.local.toml`: the file as written, and the file merged over
//! the stored config of the project it names.
//!
//! This is the one reader. [`written`] returns the text, [`stored`] a stored
//! config, and [`Local::load`], [`Local::resolve`] and [`Local::load_table`]
//! the merged result; the scope, the layers, `status` and `local` take what
//! these return and never read the file or the store themselves.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::inherit::{self, Provenance};
use crate::local::{LOCAL_FILE, Local, LocalError, STORE_DIR, check_name, parse_stored};

impl Local {
    /// Read the file beside the manifest in `dir`, merged over its project's
    /// stored config where it names one.
    ///
    /// `Ok(None)` where there is no file, which is the ordinary case. A file
    /// that is there and does not parse is an error naming it: a setting
    /// silently ignored is worse than a command that refuses.
    pub fn load(dir: &Path) -> Result<Option<Self>, LocalError> {
        let Some((path, text)) = written(dir)? else {
            return Ok(None);
        };
        Self::resolve(dir, &path, &text).map(Some)
    }

    /// `text`, as the content of the file at `path` in `dir`, read over its
    /// project's stored config. What [`Local::load`] does once the file is
    /// read, and what a writer asks of the text it is about to write.
    pub fn resolve(dir: &Path, path: &Path, text: &str) -> Result<Self, LocalError> {
        let parse_err = |source| {
            LocalError::Parse {
                path: path.to_path_buf(),
                source,
            }
        };
        let raw = Self::parse(text).map_err(parse_err)?;
        if raw.instance.project.is_none() {
            return Ok(raw);
        }
        let (table, provenance) = merge_file(dir, path, text)?;
        let mut local: Local = toml::Value::Table(table).try_into().map_err(parse_err)?;
        if local.instance.work.is_empty() {
            return Err(LocalError::EmptyWork);
        }
        local.check_console()?;
        local.provenance = provenance;
        Ok(local)
    }

    /// The parsed document of the file in `dir` after the merge, for a reader
    /// that owns tables this module does not type (the layers). `None` where
    /// there is no file. It checks the merge and nothing the schema says.
    pub fn load_table(dir: &Path) -> Result<Option<toml::Table>, LocalError> {
        let Some((path, text)) = written(dir)? else {
            return Ok(None);
        };
        merge_file(dir, &path, &text).map(|(t, _)| Some(t))
    }
}

/// The file in `dir` as written, with its path: `None` where there is none.
/// Nothing is merged, so it works on a file whose project cannot be read.
pub fn written(dir: &Path) -> Result<Option<(PathBuf, String)>, LocalError> {
    let path = dir.join(LOCAL_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some((path, text))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => {
            Err(LocalError::Io {
                path,
                source,
            })
        },
    }
}

/// The stored config `name`, as its path and text, parsed and checked. The one
/// way the store is read.
pub fn stored(dir: &Path, name: &str) -> Result<(PathBuf, String), LocalError> {
    check_name(name)?;
    let path = dir.join(STORE_DIR).join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path).map_err(|source| {
        LocalError::Io {
            path: path.clone(),
            source,
        }
    })?;
    parse_stored(&path, name, &text)?;
    Ok((path, text))
}

/// The two keys of `[instance]` the merge reads, taken from a document whose
/// other keys may be anything.
#[derive(Deserialize)]
struct Head {
    #[serde(default)]
    instance: HeadInstance,
}

#[derive(Deserialize, Default)]
struct HeadInstance {
    project: Option<String>,
    #[serde(default)]
    unset:   Vec<String>,
}

/// The file's table merged over its project's base, with the provenance where
/// it names a project, and the table as written where it does not.
fn merge_file(
    dir: &Path,
    path: &Path,
    text: &str,
) -> Result<(toml::Table, Option<Provenance>), LocalError> {
    let parse_err = |source| {
        LocalError::Parse {
            path: path.to_path_buf(),
            source,
        }
    };
    let local: toml::Table = toml::from_str(text).map_err(parse_err)?;
    let head: Head = toml::Value::Table(local.clone())
        .try_into()
        .map_err(parse_err)?;
    let Some(project) = head.instance.project else {
        return Ok((local, None));
    };
    let unusable = |path: PathBuf, reason: String| {
        LocalError::Project {
            name: project.clone(),
            path,
            reason,
        }
    };
    let (base_path, base_text) = stored(dir, &project).map_err(|e| {
        let path = dir.join(STORE_DIR).join(format!("{project}.toml"));
        unusable(path, chain(&e))
    })?;
    let base: toml::Table =
        toml::from_str(&base_text).map_err(|e| unusable(base_path, e.to_string()))?;
    inherit::merge(&project, &base, &local, &head.instance.unset)
        .map(|(t, p)| (t, Some(p)))
        .map_err(|e| LocalError::Unset(e.0))
}

/// The error and what is under it, on one line.
pub fn chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg.push_str(": ");
        msg.push_str(s.to_string().trim_end());
        src = s.source();
    }
    msg
}
