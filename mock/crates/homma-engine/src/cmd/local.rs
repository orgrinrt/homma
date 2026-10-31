//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma local ...`: the clone's own `homma.local.toml`.
//!
//! None of these loads the manifest through [`homma_core::Config`]: they need
//! only the directory the manifest sits in, and `set` has to be able to write a
//! file somebody is repairing in a workspace whose manifest does not load.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use homma_core::local::{self, LOCAL_FILE, Local};

use super::Outcome;
use crate::cli::{Cli, LocalOp, OutputFormat};

pub fn run(cli: &Cli, op: &LocalOp) -> Result<Outcome> {
    let dir = manifest_dir(cli);
    match op {
        LocalOp::Init {
            work,
        } => {
            let ignored = init(&dir, work)?;
            println!("wrote {}", dir.join(LOCAL_FILE).display());
            if ignored {
                println!("added /{LOCAL_FILE} to .gitignore");
            }
            Ok(Outcome::Ok)
        },
        LocalOp::Show {
            effective,
        } => {
            // The file as written needs no project to be readable, so the
            // command that repairs a file with a missing project still prints it.
            let as_written = matches!(cli.output, OutputFormat::Human) && !*effective;
            let found = if as_written {
                homma_core::resolve::written(&dir)?.map(|(_, text)| (None, text))
            } else {
                show(&dir)?.map(|(parsed, text)| (Some(parsed), text))
            };
            let Some((parsed, text)) = found else {
                println!(
                    "no {LOCAL_FILE} beside {}",
                    dir.join("homma.toml").display()
                );
                return Ok(Outcome::ReportedFailure);
            };
            let mut out = std::io::stdout().lock();
            match (cli.output, parsed) {
                (OutputFormat::Human, Some(parsed)) => {
                    out.write_all(toml::to_string_pretty(&parsed)?.as_bytes())?;
                },
                (OutputFormat::Human, None) => out.write_all(text.as_bytes())?,
                (OutputFormat::Json, Some(parsed)) => {
                    serde_json::to_writer_pretty(&mut out, &parsed)?;
                    writeln!(out)?;
                },
                (OutputFormat::Json, None) => unreachable!("json is always the merged document"),
            }
            Ok(Outcome::Ok)
        },
        LocalOp::List => {
            let Some((entries, installed)) = list(&dir)? else {
                match cli.output {
                    OutputFormat::Human => {
                        println!("no store at {}", dir.join(local::STORE_DIR).display());
                    },
                    OutputFormat::Json => println!("[]"),
                }
                return Ok(Outcome::ReportedFailure);
            };
            match cli.output {
                OutputFormat::Human => {
                    for e in &entries {
                        let mut line = e.name.clone();
                        if installed.as_deref() == Some(e.name.as_str()) {
                            line.push_str("  installed");
                        }
                        if let Some(err) = &e.error {
                            line.push_str(&format!("  unusable: {err}"));
                        }
                        println!("{line}");
                    }
                },
                OutputFormat::Json => {
                    let rows: Vec<_> = entries
                        .iter()
                        .map(|e| {
                            serde_json::json!({
                                "name": e.name,
                                "installed": installed.as_deref() == Some(e.name.as_str()),
                                "title": e.title,
                                "error": e.error,
                            })
                        })
                        .collect();
                    let mut out = std::io::stdout().lock();
                    serde_json::to_writer_pretty(&mut out, &rows)?;
                    writeln!(out)?;
                },
            }
            Ok(Outcome::Ok)
        },
        LocalOp::Install {
            name,
            replace,
        } => {
            let path = dir.join(LOCAL_FILE);
            match install(&dir, name, *replace)? {
                Installed::Written => {
                    println!("wrote {}, which inherits the stored {name}", path.display())
                },
                Installed::Unchanged => {
                    println!("{} already inherits the stored {name}", path.display())
                },
                Installed::Migrated {
                    dropped,
                    kept,
                } => {
                    println!(
                        "migrated {} to inherit the stored {name}: {dropped} values now inherited, {kept} kept as overrides or additions",
                        path.display()
                    );
                },
                Installed::Replaced(aside) => {
                    println!(
                        "wrote {}, which inherits the stored {name}; the file it replaced is {}",
                        path.display(),
                        aside.display()
                    );
                },
            }
            Ok(Outcome::Ok)
        },
        LocalOp::Set {
            key,
            value,
        } => {
            set(&dir, key, value)?;
            Ok(Outcome::Ok)
        },
    }
}

/// The directory holding the manifest, absolute, which is where the file sits.
pub(crate) fn manifest_dir(cli: &Cli) -> PathBuf {
    let path = super::config_path(cli);
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let absolute = if path.is_absolute() { path } else { here.join(path) };
    absolute.parent().map(Path::to_path_buf).unwrap_or(here)
}

/// Write the skeleton and ignore it. `true` when `.gitignore` was touched.
pub fn init(dir: &Path, work: &str) -> Result<bool> {
    if !dir.join("homma.toml").is_file() {
        bail!(
            "no homma.toml in {}; the file sits beside the manifest",
            dir.display()
        );
    }
    let path = dir.join(LOCAL_FILE);
    if path.exists() {
        bail!("{} exists; `homma local set` changes it", path.display());
    }
    std::fs::write(&path, local::skeleton(work))
        .with_context(|| format!("writing {}", path.display()))?;
    local::ensure_ignored(dir).context("adding the file to .gitignore")
}

/// What `install` did.
#[derive(Debug, PartialEq)]
pub enum Installed {
    /// There was no file; the one-line project file was written.
    Written,
    /// The file already names this project.
    Unchanged,
    /// A full copy of the same work became a file that inherits it: this many
    /// values are inherited now, and this many stay.
    Migrated {
        dropped: usize,
        kept:    usize,
    },
    /// The old file was moved here and the project file written.
    Replaced(PathBuf),
}

/// One stored config, as `list` shows it.
#[derive(Debug, PartialEq)]
pub struct Stored {
    pub name:  String,
    pub title: Option<String>,
    /// Why it cannot be installed, where it cannot.
    pub error: Option<String>,
}

/// A stored name is a file stem and nothing that could leave the store.
fn check_name(name: &str) -> Result<()> {
    Ok(local::check_name(name)?)
}

/// The stored configs, sorted, and the work of the installed file where it
/// loads. `None` where the store does not exist.
pub fn list(dir: &Path) -> Result<Option<(Vec<Stored>, Option<String>)>> {
    let store = dir.join(local::STORE_DIR);
    let read = match std::fs::read_dir(&store) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", store.display())),
    };
    let mut entries = Vec::new();
    for item in read {
        let path = item?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        // A name `install` would refuse, and one that is not UTF-8, are listed with
        // the reason rather than as usable or left out.
        let stem = path.file_stem().unwrap_or_default();
        let name = stem.to_string_lossy().into_owned();
        let refused = match stem.to_str() {
            Some(s) => check_name(s).err().map(|e| e.to_string()),
            None => Some("the file name is not UTF-8".to_owned()),
        };
        if refused.is_some() {
            entries.push(Stored {
                name,
                title: None,
                error: refused,
            });
            continue;
        }
        let checked = homma_core::resolve::stored(dir, &name)
            .map(|(_, text)| text)
            .and_then(|text| local::parse_stored(&path, &name, &text))
            .map_err(|e| homma_core::resolve::chain(&e));
        entries.push(match checked {
            Ok(l) => {
                Stored {
                    name,
                    title: l.instance.title,
                    error: None,
                }
            },
            Err(error) => {
                Stored {
                    name,
                    title: None,
                    error: Some(error),
                }
            },
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    // The name the installed file stands for, read as written: the project it
    // names, or for a copy the work it is for. Not the merged document, which
    // fails where the project is the thing that is missing.
    let installed = homma_core::resolve::written(dir)
        .ok()
        .flatten()
        .and_then(|(_, t)| Local::parse(&t).ok())
        .map(|l| l.instance.project.unwrap_or(l.instance.work));
    Ok(Some((entries, installed)))
}

/// Make the clone inherit the stored config `name`.
///
/// No copy and no link: the file written names the project and the stored
/// config is read under it by [`Local::load`], so a change to the store reaches
/// the clone. Under the lock `set` takes, so neither lands on the other's half.
pub fn install(dir: &Path, name: &str, replace: bool) -> Result<Installed> {
    if !dir.join("homma.toml").is_file() {
        bail!(
            "no homma.toml in {}; the file sits beside the manifest",
            dir.display()
        );
    }
    let (_, stored) = match homma_core::resolve::stored(dir, name) {
        Ok(found) => found,
        Err(local::LocalError::Io {
            source,
            ..
        }) if source.kind() == std::io::ErrorKind::NotFound => {
            let names = list(dir)?
                .map(|(es, _)| {
                    es.into_iter()
                        .map(|e| e.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            bail!(
                "no stored config `{name}` in {}; the names are: {}",
                dir.join(local::STORE_DIR).display(),
                if names.is_empty() { "none".to_owned() } else { names }
            );
        },
        Err(e) => return Err(e.into()),
    };
    let fresh = local::project_skeleton(name);

    let path = dir.join(LOCAL_FILE);
    let _lock = Lock::take(dir.join(format!("{LOCAL_FILE}.lock")), LOCK_WAIT)?;
    let done = match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_atomic(dir, &path, &fresh)?;
            Installed::Written
        },
        Err(e) => return Err(e).with_context(|| format!("looking at {}", path.display())),
        Ok(meta) if meta.file_type().is_symlink() => {
            if !replace {
                bail!(
                    "{} is a link; a tool's write replaces a link with a plain file, and writes through \
                     one would edit what it points at. `--replace` moves it aside and \
                     installs a file that inherits `{name}`",
                    path.display()
                );
            }
            replace_with(dir, &path, &fresh)?
        },
        Ok(_) => {
            let (_, text) = homma_core::resolve::written(dir)?
                .with_context(|| format!("{} vanished", path.display()))?;
            match Local::parse(&text) {
                Ok(l) if l.instance.project.as_deref() == Some(name) => Installed::Unchanged,
                Ok(l) if l.instance.project.is_none() && l.instance.work == name => {
                    let m = homma_core::migrate::migrate(&text, &stored, name)?;
                    // The merged file must load: a dashboard kept here beside a
                    // console the store names is the contradiction, refused.
                    Local::resolve(dir, &path, &m.text)?;
                    write_atomic(dir, &path, &m.text)?;
                    Installed::Migrated {
                        dropped: m.dropped.len(),
                        kept:    m.kept.len(),
                    }
                },
                _ if replace => replace_with(dir, &path, &fresh)?,
                Ok(l) => {
                    let of = l.instance.project.unwrap_or(l.instance.work);
                    bail!(
                        "{} is for `{of}`, not `{name}`; `--replace` moves it aside and \
                         installs a file that inherits `{name}`",
                        path.display()
                    )
                },
                Err(e) => {
                    bail!(
                        "{} does not parse ({e}); fix it, or `--replace` moves it aside \
                         and installs a file that inherits `{name}`",
                        path.display()
                    )
                },
            }
        },
    };
    local::ensure_ignored(dir).context("adding the file to .gitignore")?;
    Ok(done)
}

/// Install `text` over an existing file, keeping the old one aside.
///
/// The new text is written to its temporary file first, so a write that fails
/// leaves the old file at its path and nothing aside. Only then is the old file
/// moved aside, and the temporary file renamed into its place; if that last
/// rename fails the old file is moved back.
fn replace_with(dir: &Path, path: &Path, text: &str) -> Result<Installed> {
    let tmp = stage(dir, text)?;
    let aside = match move_aside(dir, path) {
        Ok(aside) => aside,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        },
    };
    if let Err(e) = commit(&tmp, path) {
        let _ = std::fs::rename(&aside, path);
        return Err(e);
    }
    Ok(Installed::Replaced(aside))
}

/// Rename the file to `homma.local.toml.replaced`, refusing to overwrite one.
fn move_aside(dir: &Path, path: &Path) -> Result<PathBuf> {
    let aside = dir.join(format!("{LOCAL_FILE}.replaced"));
    if std::fs::symlink_metadata(&aside).is_ok() {
        bail!(
            "{} is already there, an earlier replaced file; remove it first",
            aside.display()
        );
    }
    std::fs::rename(path, &aside).with_context(|| format!("moving {} aside", path.display()))?;
    Ok(aside)
}

/// Write the text to a temporary file in `dir`, which `commit` renames into place.
fn stage(dir: &Path, text: &str) -> Result<PathBuf> {
    let tmp = dir.join(format!("{LOCAL_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    Ok(tmp)
}

/// Rename a staged file over `path`, removing it where that fails.
fn commit(tmp: &Path, path: &Path) -> Result<()> {
    std::fs::rename(tmp, path).with_context(|| {
        let _ = std::fs::remove_file(tmp);
        format!("replacing {}", path.display())
    })
}

/// Write through a temporary file in the same directory renamed over `path`.
fn write_atomic(dir: &Path, path: &Path, text: &str) -> Result<()> {
    commit(&stage(dir, text)?, path)
}

/// The file parsed and as written, or `None` where there is none.
pub fn show(dir: &Path) -> Result<Option<(Local, String)>> {
    let Some(parsed) = Local::load(dir)? else {
        return Ok(None);
    };
    let text = homma_core::resolve::written(dir)?
        .map(|(_, t)| t)
        .unwrap_or_default();
    Ok(Some((parsed, text)))
}

/// How long `set` waits for another writer's lock before refusing.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The lock beside the file, removed when dropped.
struct Lock(PathBuf);

impl Lock {
    /// Create the lock file exclusively, retrying until [`LOCK_WAIT`] is spent.
    fn take(path: PathBuf, wait: std::time::Duration) -> Result<Self> {
        let start = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if start.elapsed() >= wait {
                        bail!(
                            "{} is held; another `homma local set` is writing, or one died \
                             holding it and the file can be removed",
                            path.display()
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                },
                Err(e) => return Err(e).with_context(|| format!("creating {}", path.display())),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Write one string into the file, refusing where there is no file.
///
/// Under a lock beside the file, so two writers both land, and through a
/// temporary file renamed over the original, so a reader sees the old file or
/// the new one and never a truncated half.
pub fn set(dir: &Path, key: &str, value: &str) -> Result<()> {
    set_waiting(dir, key, value, LOCK_WAIT)
}

fn set_waiting(dir: &Path, key: &str, value: &str, wait: std::time::Duration) -> Result<()> {
    let path = dir.join(LOCAL_FILE);
    let _lock = Lock::take(dir.join(format!("{LOCAL_FILE}.lock")), wait)?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "no {}; `homma local init --work <name>` makes one",
                path.display()
            )
        },
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let out = local::set(&text, key, value)?;
    // The check on the file alone cannot see a dashboard the project holds.
    // Any other trouble resolving it is the file's existing state, reported
    // where it is read, and must not stop a repair.
    if let Err(e @ local::LocalError::DashboardWhereConsole(_)) = Local::resolve(dir, &path, &out) {
        return Err(e.into());
    }
    write_atomic(dir, &path, &out)
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
