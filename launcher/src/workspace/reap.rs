//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Removing a workspace, refusing while it still holds work, and listing
//! what is there.

use std::io::Write;
use std::path::Path;

use super::status;

/// Remove the workspace at `root`.
///
/// A spawned workspace is the only copy of whatever was done in it, so every
/// repository in it is surveyed first and anything dirty or on no remote
/// refuses, naming what it found. `force` removes anyway, having printed
/// the same list, so what is discarded is at least on the screen.
pub fn reap(root: &Path, force: bool, out: &mut dyn Write) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("no workspace at {}", root.display()));
    }
    let members = status::survey(root)?;
    let held: Vec<_> = members.iter().filter(|m| m.holds_work()).collect();
    if !held.is_empty() {
        let mut why = String::new();
        for m in &held {
            if m.dirty {
                why.push_str(&format!("  {}: uncommitted or untracked changes\n", m.name));
            }
            if !m.unpushed.is_empty() {
                why.push_str(&format!(
                    "  {}: {} commit(s) on no remote\n",
                    m.name,
                    m.unpushed.len()
                ));
                for line in &m.unpushed {
                    why.push_str(&format!("      {line}\n"));
                }
            }
        }
        if !force {
            return Err(format!(
                "refusing to remove {}:\n{why}push or discard the above first, or pass --force \
                 having read it",
                root.display()
            ));
        }
        out.write_all(format!("--force given; removing anyway:\n{why}").as_bytes())
            .map_err(|e| e.to_string())?;
    }
    std::fs::remove_dir_all(root).map_err(|e| format!("cannot remove {}: {e}", root.display()))?;
    out.write_all(format!("removed {}\n", root.display()).as_bytes())
        .map_err(|e| e.to_string())
}

/// The directories under the workspaces root, one per line, or a line saying
/// there are none.
pub fn list(workspaces_root: &Path, out: &mut dyn Write) -> Result<(), String> {
    let w = |out: &mut dyn Write, s: String| out.write_all(s.as_bytes()).map_err(|e| e.to_string());
    if !workspaces_root.is_dir() {
        return w(
            out,
            format!(
                "no workspaces ({} does not exist)\n",
                workspaces_root.display()
            ),
        );
    }
    let mut names: Vec<String> = std::fs::read_dir(workspaces_root)
        .map_err(|e| format!("cannot read {}: {e}", workspaces_root.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if names.is_empty() {
        return w(
            out,
            format!("no workspaces in {}\n", workspaces_root.display()),
        );
    }
    for n in names {
        w(out, format!("{n}\n"))?;
    }
    Ok(())
}
