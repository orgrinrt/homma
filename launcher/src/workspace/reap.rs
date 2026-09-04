//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Removing a workspace, refusing while it still holds work, and listing
//! what is there.

use std::io::Write;
use std::path::Path;

use super::{git, status};
use crate::settings::Prefs;

/// Remove the workspace at `root`.
///
/// A spawned workspace is the only copy of whatever was done in it, so every
/// repository in it is surveyed first and anything dirty or on no remote
/// refuses, naming what it found. `force` removes anyway, having printed
/// the same list, so what is discarded is at least on the screen.
///
/// Three refusals come ahead of the survey and `force` lifts none of them:
/// the target is not a workspace at all, which is a directory with no clone
/// at its root; it is a directory the settings deny, since a verb that ends
/// in `remove_dir_all` is not held to a lower bar than the one that clones;
/// and `cwd` is inside it, since removing the directory a shell stands in
/// leaves that shell where every later command fails for no visible reason.
/// Worktrees, a `.git` file rather than a directory, are what the survey
/// cannot see, so any found one level down or under `.worktrees/` refuse
/// too, whatever `force` says: a worktree is somebody's seat.
pub fn reap(
    prefs: &Prefs,
    home: Option<&Path>,
    cwd: &Path,
    root: &Path,
    force: bool,
    out: &mut dyn Write,
) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("no workspace at {}", root.display()));
    }
    if !git::is_clone(root) {
        return Err(format!(
            "{} holds no repository at its root, so it is not a workspace and is not removed",
            root.display()
        ));
    }
    if let Some(why) = prefs.refusal_for(root, home)? {
        return Err(why);
    }
    let resolved = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let standing = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if standing.starts_with(&resolved) {
        return Err(format!(
            "the cwd is inside {}, and removing the directory a shell stands in leaves it \
             nowhere; `cd` out of it first and name it: `homma workspace reap <slug>`",
            root.display()
        ));
    }
    let seats = worktrees_in(root)?;
    if !seats.is_empty() {
        return Err(format!(
            "{} holds worktrees, which are somebody's seats and which the survey cannot \
             see:\n{}remove them with `git worktree remove` first",
            root.display(),
            seats
                .iter()
                .map(|s| format!("  {}\n", s.display()))
                .collect::<String>()
        ));
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

/// Every worktree in a workspace: an entry one level down whose `.git` is a
/// file, and every entry under `.worktrees/`, where the workspace rules put
/// agent seats.
fn worktrees_in(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut found = Vec::new();
    let entries = |dir: &Path| -> Result<Vec<std::path::PathBuf>, String> {
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        Ok(std::fs::read_dir(dir)
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect())
    };
    for p in entries(root)? {
        if p.join(".git").is_file() {
            found.push(p);
        }
    }
    for p in entries(&root.join(".worktrees"))? {
        if p.is_dir() {
            found.push(p);
        }
    }
    found.sort();
    Ok(found)
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
