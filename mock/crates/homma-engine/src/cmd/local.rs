//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma local ...`: the clone's own `homma.local.toml`.
//!
//! None of these loads the manifest through [`homma_core::Config`], because
//! that load fails on a malformed local file, and `set` has to be able to
//! write the file somebody is trying to repair. They need only the directory
//! the manifest sits in.

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
fn manifest_dir(cli: &Cli) -> PathBuf {
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

/// Write one string into the file, refusing where there is no file.
pub fn set(dir: &Path, key: &str, value: &str) -> Result<()> {
    let path = dir.join(LOCAL_FILE);
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
    std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
