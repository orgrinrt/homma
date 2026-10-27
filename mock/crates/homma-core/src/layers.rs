//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The layers: the overlay files `layers` in `homma.toml` names, read for the
//! two hooks tables and nothing else. `homma.toml` is the base, each overlay
//! appends to it in list order, and no layer replaces another's entries.
//!
//! An overlay is `homma.<name>.toml` in the workspace root. `homma.local.toml`
//! has a reader of its own for its own tables, so for that file the tables
//! this module does not read are left to it; any other overlay carries the two
//! hooks tables only, and a table beside them is refused by name, since
//! nothing would read it and a setting silently ignored is worse than a
//! command that refuses.

use std::fmt;
use std::path::{Path, PathBuf};

use homma_api::layers::{DEFAULT_LAYER, MANIFEST, is_layer_name};
use homma_api::{AgentHooks, Hooks, Layers};

/// What one overlay file declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    pub hooks: Hooks,
    pub agent: AgentHooks,
}

/// Why an overlay was not read.
#[derive(Debug)]
pub enum LayerError {
    Io {
        path:   PathBuf,
        source: std::io::Error,
    },
    /// The text is not toml, or a table of it is not a valid hooks table.
    Parse {
        path:   PathBuf,
        source: toml::de::Error,
    },
    /// An overlay that is not the clone's own carrying a table nothing reads.
    Table {
        path:  PathBuf,
        table: String,
    },
    /// `[agent]` carrying something other than `hooks`.
    AgentKey {
        path: PathBuf,
        key:  String,
    },
}

impl fmt::Display for LayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayerError::Io {
                path,
                ..
            } => write!(f, "failed to read the layer {}", path.display()),
            LayerError::Parse {
                path,
                ..
            } => write!(f, "the layer {} does not parse", path.display()),
            LayerError::Table {
                path,
                table,
            } => {
                write!(
                    f,
                    "the layer {} carries `[{table}]`, which nothing reads from an overlay; an \
                     overlay holds `[hooks]` and `[agent.hooks]`",
                    path.display()
                )
            },
            LayerError::AgentKey {
                path,
                key,
            } => {
                write!(
                    f,
                    "the layer {} carries `[agent.{key}]`; `[agent]` holds `hooks` and nothing else",
                    path.display()
                )
            },
        }
    }
}

impl LayerError {
    /// The line a verb prints: what failed, and the reader's own reason after it
    /// where there is one, which for a parse is the line and column. `Display`
    /// alone names the file and drops the cause, and `homma local show` gives
    /// the cause for the same file, so a report without it sends the operator to
    /// a second command for the one thing they needed.
    pub fn report(&self) -> String {
        match std::error::Error::source(self) {
            Some(cause) => format!("{self}: {cause}"),
            None => self.to_string(),
        }
    }
}

impl std::error::Error for LayerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LayerError::Io {
                source,
                ..
            } => Some(source),
            LayerError::Parse {
                source,
                ..
            } => Some(source),
            LayerError::Table {
                ..
            }
            | LayerError::AgentKey {
                ..
            } => None,
        }
    }
}

/// Read one overlay from its text. `own_tables` is true for the file another
/// reader owns the rest of, which is `homma.local.toml`.
pub fn parse(path: &Path, text: &str, own_tables: bool) -> Result<Overlay, LayerError> {
    let parse_err = |source| {
        LayerError::Parse {
            path: path.to_path_buf(),
            source,
        }
    };
    let mut table: toml::Table = toml::from_str(text).map_err(parse_err)?;
    let hooks = match table.remove("hooks") {
        Some(v) => v.try_into::<Hooks>().map_err(parse_err)?,
        None => Hooks::default(),
    };
    let agent = match table.remove("agent") {
        Some(toml::Value::Table(mut a)) => {
            let hooks = match a.remove("hooks") {
                Some(v) => v.try_into::<AgentHooks>().map_err(parse_err)?,
                None => AgentHooks::default(),
            };
            if let Some(key) = a.keys().next() {
                return Err(LayerError::AgentKey {
                    path: path.to_path_buf(),
                    key:  key.clone(),
                });
            }
            hooks
        },
        Some(other) => {
            // an `agent` that is not a table is refused by the same words a
            // table of the wrong shape gets, from the type that reads it
            other
                .try_into::<homma_api::AgentTable>()
                .map_err(parse_err)?
                .hooks
        },
        None => AgentHooks::default(),
    };
    if !own_tables {
        if let Some(table) = table.keys().next() {
            return Err(LayerError::Table {
                path:  path.to_path_buf(),
                table: table.clone(),
            });
        }
    }
    Ok(Overlay {
        hooks,
        agent,
    })
}

/// Read the overlay `name` in `dir`, or `None` where there is no such file,
/// which is the ordinary case for a clone with no `homma.local.toml`.
pub fn read(dir: &Path, name: &str) -> Result<Option<Overlay>, LayerError> {
    let path = dir.join(name);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(LayerError::Io {
                path,
                source,
            });
        },
    };
    parse(&path, &text, name == DEFAULT_LAYER).map(Some)
}

/// Append every named overlay in `dir` to the two tables, in list order, and
/// return the ones that could not be read, in the same order.
///
/// An overlay that fails contributes nothing and does not stop the others. The
/// load that calls this keeps going on the failures, since every command loads
/// the manifest and `homma local` has to run to repair a broken
/// `homma.local.toml`; what must not go on is a hook run or an install, which
/// refuse while any failure is on the config, because a hook silently dropped
/// is a check that did not happen.
pub fn apply(
    dir: &Path,
    layers: &Layers,
    hooks: &mut Hooks,
    agent: &mut AgentHooks,
) -> Vec<LayerError> {
    let mut failed = Vec::new();
    for name in layers.names() {
        match read(dir, name) {
            Ok(Some(overlay)) => {
                hooks.append(&overlay.hooks);
                agent.append(&overlay.agent);
            },
            Ok(None) => {},
            Err(e) => failed.push(e),
        }
    }
    failed
}

/// Which of the workspace root's `homma.*.toml` files are read, which are
/// named and absent, and which are present and named by nobody.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Survey {
    /// Named and present, in the order applied.
    pub read:   Vec<String>,
    /// Named and not there.
    pub absent: Vec<String>,
    /// Present, shaped like an overlay, and not named: never read.
    pub strays: Vec<String>,
}

/// Survey `dir` against `layers`.
pub fn survey(dir: &Path, layers: &Layers) -> std::io::Result<Survey> {
    let mut out = Survey::default();
    for name in layers.names() {
        if dir.join(name).is_file() {
            out.read.push(name.clone());
        } else {
            out.absent.push(name.clone());
        }
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name != MANIFEST
            && is_layer_name(&name)
            && !layers.names_file(&name)
            && entry.path().is_file()
        {
            out.strays.push(name);
        }
    }
    out.strays.sort();
    Ok(out)
}

#[cfg(test)]
mod tests;
