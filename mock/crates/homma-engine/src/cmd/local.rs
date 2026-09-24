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
        LocalOp::Show => {
            let Some((parsed, text)) = show(&dir)? else {
                println!(
                    "no {LOCAL_FILE} beside {}",
                    dir.join("homma.toml").display()
                );
                return Ok(Outcome::ReportedFailure);
            };
            let mut out = std::io::stdout().lock();
            match cli.output {
                OutputFormat::Human => out.write_all(text.as_bytes())?,
                OutputFormat::Json => {
                    serde_json::to_writer_pretty(&mut out, &parsed)?;
                    writeln!(out)?;
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

/// The file parsed and as written, or `None` where there is none.
pub fn show(dir: &Path) -> Result<Option<(Local, String)>> {
    let Some(parsed) = Local::load(dir)? else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(dir.join(LOCAL_FILE))?;
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
    let tmp = dir.join(format!("{LOCAL_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, out).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| {
        let _ = std::fs::remove_file(&tmp);
        format!("replacing {}", path.display())
    })
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
