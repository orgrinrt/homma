//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The layers: the overlay files `layers` in `homma.toml` names, read for the
//! agent hooks and the nudges, and nothing else. `homma.toml` is the base, each overlay
//! appends to it in list order, and no layer replaces another's entries.
//!
//! An overlay is `homma.<name>.toml` beside the manifest. `homma.local.toml`
//! has a reader of its own for its own tables, so for that file the tables
//! this module does not read are left to it; any other overlay carries the two
//! agent tables only, and a table beside them is refused by name, since
//! nothing would read it and a setting silently ignored is worse than a
//! command that refuses.

use std::fmt;
use std::path::{Path, PathBuf};

use homma_api::layers::{DEFAULT_LAYER, MANIFEST};
use homma_api::{AgentHooks, AgentTable, InvalidNudge, Layers, Nudges};

use crate::local::{Local, LocalError};

/// What one overlay file declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// The retired tables the file still carries, taken out unread.
    pub retired: Vec<crate::config::Retired>,
    pub agent:   AgentTable,
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
    /// The file inherits a project that cannot be read, so what it adds to the
    /// project's tables is unknown.
    Inherit {
        path:   PathBuf,
        reason: String,
    },
    /// `[agent]` carrying something other than `hooks` and `nudges`.
    AgentKey {
        path: PathBuf,
        key:  String,
    },
    /// A layer's nudges that cannot join the ones read before it, which is a
    /// name a lower layer already has.
    Nudges {
        path:   PathBuf,
        source: InvalidNudge,
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
            LayerError::Inherit {
                path,
                reason,
            } => {
                write!(
                    f,
                    "the layer {} cannot be read over its project: {reason}",
                    path.display()
                )
            },
            LayerError::Table {
                path,
                table,
            } => {
                write!(
                    f,
                    "the layer {} carries `[{table}]`, which nothing reads from an overlay; an \
                     overlay holds `[agent.hooks]` and `[agent.nudges]`",
                    path.display()
                )
            },
            LayerError::AgentKey {
                path,
                key,
            } => {
                write!(
                    f,
                    "the layer {} carries `[agent.{key}]`; `[agent]` holds `hooks` and `nudges` \
                     and nothing else",
                    path.display()
                )
            },
            LayerError::Nudges {
                path,
                ..
            } => {
                write!(
                    f,
                    "the layer {} adds nudges that cannot be read",
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
            LayerError::Nudges {
                source,
                ..
            } => Some(source),
            LayerError::Table {
                ..
            }
            | LayerError::Inherit {
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
    // A `[hooks]` left from when git hooks were declared here is named, not
    // refused, the same as in the manifest.
    let file = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let retired = crate::config::retired::take(&mut table, &file);
    let agent = match table.remove("agent") {
        Some(toml::Value::Table(mut a)) => {
            let hooks = match a.remove("hooks") {
                Some(v) => v.try_into::<AgentHooks>().map_err(parse_err)?,
                None => AgentHooks::default(),
            };
            let nudges = match a.remove("nudges") {
                Some(v) => v.try_into::<Nudges>().map_err(parse_err)?,
                None => Nudges::default(),
            };
            if let Some(key) = a.keys().next() {
                return Err(LayerError::AgentKey {
                    path: path.to_path_buf(),
                    key:  key.clone(),
                });
            }
            AgentTable {
                hooks,
                nudges,
            }
        },
        Some(other) => {
            // an `agent` that is not a table is refused by the same words a
            // table of the wrong shape gets, from the type that reads it
            other.try_into::<AgentTable>().map_err(parse_err)?
        },
        None => AgentTable::default(),
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
        retired,
        agent,
    })
}

/// Read the overlay `name` in `dir`, or `None` where there is no such file,
/// which is the ordinary case for a clone with no `homma.local.toml`.
pub fn read(dir: &Path, name: &str) -> Result<Option<Overlay>, LayerError> {
    let path = dir.join(name);
    if name == DEFAULT_LAYER {
        // The clone's own file is read by its one reader, which merges it over
        // the stored config of the project it names.
        return match Local::load_table(dir) {
            Ok(None) => Ok(None),
            Ok(Some(table)) => {
                let text = toml::to_string(&table).map_err(|e| {
                    LayerError::Inherit {
                        path:   path.clone(),
                        reason: e.to_string(),
                    }
                })?;
                parse(&path, &text, true).map(Some)
            },
            Err(LocalError::Parse {
                source,
                ..
            }) => {
                Err(LayerError::Parse {
                    path,
                    source,
                })
            },
            Err(LocalError::Io {
                source,
                ..
            }) => {
                Err(LayerError::Io {
                    path,
                    source,
                })
            },
            Err(e) => {
                Err(LayerError::Inherit {
                    path,
                    reason: e.to_string(),
                })
            },
        };
    }
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

/// What applying the overlays came to.
#[derive(Debug, Default)]
pub struct Applied {
    /// The overlays that could not be read, in list order.
    pub failed:  Vec<LayerError>,
    /// The retired tables the overlays that were read still carry.
    pub retired: Vec<crate::config::Retired>,
}

/// Append every named overlay in `dir` to the agent tables, in list order, and
/// return the ones that could not be read, in the same order, with the retired
/// tables the rest still carry.
///
/// An overlay that fails contributes nothing and does not stop the others. The
/// load that calls this keeps going on the failures, since every command loads
/// the manifest and `homma local` has to run to repair a broken
/// `homma.local.toml`; what must not go on is `agent regen`, which refuses
/// while any failure is on the config, because a hook silently dropped is a
/// check that did not happen.
pub fn apply(dir: &Path, layers: &Layers, agent: &mut AgentTable) -> Applied {
    let mut out = Applied::default();
    for name in layers.names() {
        match read(dir, name) {
            Ok(Some(overlay)) => {
                // the nudges are checked first, so a layer refused for them adds
                // nothing at all rather than its hooks without its nudges
                if let Err(source) = agent.nudges.append(&overlay.agent.nudges) {
                    out.failed.push(LayerError::Nudges {
                        path: dir.join(name),
                        source,
                    });
                    continue;
                }
                agent.hooks.append(&overlay.agent.hooks);
                out.retired.extend(overlay.retired);
            },
            Ok(None) => {},
            Err(e) => out.failed.push(e),
        }
    }
    out
}

/// Which of the `homma.*.toml` files beside the manifest are read, which are
/// named and absent, and which are present and named by nobody.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Survey {
    /// Named and present, in the order applied.
    pub read:         Vec<String>,
    /// Named and not there.
    pub absent:       Vec<String>,
    /// Present as `homma.<anything>.toml`, and not named: never read. An
    /// editor's copy with a dotted middle is one, since it is the file most
    /// likely to be mistaken for a layer.
    pub strays:       Vec<String>,
    /// The clone's own file is there and `layers` does not name it. Its hooks
    /// tables are not read, its own tables still are, so it is not a stray.
    pub own_unlisted: bool,
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
            && name.starts_with("homma.")
            && name.ends_with(".toml")
            && !layers.names_file(&name)
            && entry.path().is_file()
        {
            if name == DEFAULT_LAYER {
                out.own_unlisted = true;
            } else {
                out.strays.push(name);
            }
        }
    }
    out.strays.sort();
    Ok(out)
}

#[cfg(test)]
mod tests;
