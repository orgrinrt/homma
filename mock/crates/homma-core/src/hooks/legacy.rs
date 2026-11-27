//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//! What an older homma left in a clone: one entrypoint per event in git's own
//! hooks directory calling `homma hook run <event>`, and git-shook's copies of
//! it, the kept file it adopted as `00-existing` and the declined record of one
//! it left in place. The install takes each away where it is exactly the text
//! homma wrote, since kept it would run beside the new gate; anything else at
//! those paths is somebody's and stays.

use std::path::{Path, PathBuf};

use super::HookError;
use super::shook::common_dir;

/// The entrypoint an older homma wrote for `event`, byte for byte.
pub fn entrypoint(event: &str) -> String {
    format!(
        "#!/bin/sh\n# homma writes this file; what it runs is [hooks] in homma.toml\nexec homma hook run {event} \"$@\"\n"
    )
}

/// Whether the file at `path`, named for its event, is exactly an older
/// homma's entrypoint.
fn is_entrypoint(path: &Path) -> bool {
    let Some(event) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    std::fs::read(path).is_ok_and(|bytes| bytes == entrypoint(event).as_bytes())
}

/// The files of `dir` that are an older homma's entrypoint, sorted.
fn entrypoints_in(dir: &Path) -> Result<Vec<PathBuf>, HookError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_file() && is_entrypoint(&path) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// What an older homma left at `root`: its entrypoints in git's own hooks
/// directory, and the copies git-shook kept of them.
pub fn left_behind(root: &Path) -> Result<Vec<PathBuf>, HookError> {
    let common = common_dir(root)?;
    let mut found = entrypoints_in(&common.join("hooks"))?;
    found.extend(entrypoints_in(&common.join("shook").join("kept"))?);
    Ok(found)
}

/// A file the install took away, kept so a refusal after it can put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub path: PathBuf,
    bytes:    Vec<u8>,
    mode:     std::fs::Permissions,
}

/// Put back what [`clear`] took away, as it was.
pub fn restore(removed: &[Removed]) {
    for r in removed {
        if std::fs::write(&r.path, &r.bytes).is_ok() {
            let _ = std::fs::set_permissions(&r.path, r.mode.clone());
        }
    }
}

fn take(path: &Path) -> Result<Removed, HookError> {
    let removed = Removed {
        path:  path.to_path_buf(),
        bytes: std::fs::read(path)?,
        mode:  std::fs::metadata(path)?.permissions(),
    };
    std::fs::remove_file(path)?;
    Ok(removed)
}

/// Take away what an older homma left at `root`, and git-shook's records of
/// it: the `00-existing` entry that ran a kept copy, and the declined copy
/// beside an entrypoint left in place. Returns what was removed.
pub fn clear(root: &Path) -> Result<Vec<Removed>, HookError> {
    let common = common_dir(root)?;
    let registry = common.join("shook");
    let mut removed = Vec::new();
    for path in left_behind(root)? {
        let Some(event) = path.file_name().map(PathBuf::from) else {
            continue;
        };
        removed.push(take(&path)?);
        let record = if path.parent() == Some(registry.join("kept").as_path()) {
            registry.join("entries").join(&event).join("00-existing")
        } else {
            registry.join("declined").join(&event)
        };
        // the record goes with the file only where it is the same text, or is
        // the entry that ran it, which a hand cannot have written
        let ours = record.ends_with("00-existing") || is_entrypoint(&record);
        if ours && record.is_file() {
            removed.push(take(&record)?);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
