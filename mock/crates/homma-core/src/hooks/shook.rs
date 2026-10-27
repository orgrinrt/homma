//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Registering homma's hooks with git-shook.
//!
//! git-shook owns `core.hooksPath` for a dispatcher directory of its own and
//! runs what is registered with it, so homma is at a git event as a registered
//! entry beside the others and not as an entrypoint of its own. The entry is
//! `homma hook run <event>` for every event the merged hooks table names, so
//! the layers decide which events have an entry and `homma hook run` reads the
//! layers again when the event fires to decide what it runs.
//!
//! The registration is a manifest homma generates into the common git
//! directory, `shook/manifests/homma.toml`, which git-shook reads beside the
//! tracked `shook.toml` files. A tracked manifest cannot carry a hook declared
//! only in `homma.local.toml`, which is per clone and untracked; one in the git
//! directory is per clone as well. Running `homma hook install` is the consent
//! git-shook asks for, so the install runs `git shook install --yes` itself.

use std::path::{Path, PathBuf};

use homma_api::Hooks;

use super::install::{
    HookError,
    Installed,
    Reach,
    Written,
    common_dir,
    hooks_dir as git_hooks_dir,
    script,
};
use crate::release::sh;

/// The name homma registers under, which is the manifest's name and the file
/// each entry sits in. A tracked `shook.toml` of the same name is replaced by
/// this one, so the registry holds one entry per event and not two.
pub const NAME: &str = "homma";

/// What the clone-local manifest says it is, at its head.
const HEADER: &str = "# homma writes this file, and `homma hook install` rewrites it from every layer\n\
                      # of the hooks table. Edit `[hooks]` in homma.toml or a layer it names.\n";

/// The directory git-shook generates its dispatchers into, which is what it
/// points `core.hooksPath` at. Compared against rather than matched by
/// spelling, since a repository is free to keep a directory called `shook`
/// and mean nothing by it.
pub fn hooks_dir(root: &Path) -> Result<PathBuf, HookError> {
    Ok(common_dir(root)?.join("shook").join("hooks"))
}

/// Where git-shook reads the manifests this clone keeps to itself.
pub fn manifests_dir(root: &Path) -> Result<PathBuf, HookError> {
    Ok(common_dir(root)?.join("shook").join("manifests"))
}

/// The registry file for homma's entry at `event`.
pub fn entry_path(root: &Path, event: &str) -> Result<PathBuf, HookError> {
    Ok(common_dir(root)?
        .join("shook")
        .join("entries")
        .join(event)
        .join(NAME))
}

/// The manifest text: one `homma hook run <event>` for every event the table
/// names, in name order. Git's arguments reach the command because the
/// dispatcher appends them to the argument list.
pub fn manifest(hooks: &Hooks) -> String {
    let mut out = String::from(HEADER);
    out.push_str(&format!("name = \"{NAME}\"\n"));
    for event in hooks.events() {
        out.push_str(&format!(
            "\n[hooks.{event}]\nrun = [\"homma\", \"hook\", \"run\", \"{event}\"]\n"
        ));
    }
    out
}

/// Whether homma's entry for `event` is registered, which is what makes
/// git-shook's dispatcher reach it.
pub fn reach(root: &Path, event: &str) -> Result<Reach, HookError> {
    let entry = entry_path(root, event)?;
    if entry.is_file() {
        Ok(Reach::Shook(entry))
    } else {
        Ok(Reach::ShookUnregistered(hooks_dir(root)?))
    }
}

/// Write the manifest and have git-shook register it.
pub fn install(root: &Path, hooks: &Hooks) -> Result<Installed, HookError> {
    install_with(root, hooks, None)
}

/// [`install`], with `on_path` put at the front of the child's `PATH` where
/// git-shook is not on the caller's.
pub fn install_with(
    root: &Path,
    hooks: &Hooks,
    on_path: Option<&Path>,
) -> Result<Installed, HookError> {
    let dir = manifests_dir(root)?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{NAME}.toml")), manifest(hooks))?;

    let path = on_path.map(|p| {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let mut joined = std::ffi::OsString::from(p);
        joined.push(":");
        joined.push(rest);
        joined.to_string_lossy().into_owned()
    });
    // `--only`: running this verb is the consent for homma's own manifest, and
    // not for a tracked `shook.toml` that arrived through a merge and has not
    // been looked at
    let args = ["shook", "install", "--yes", "--only", NAME];
    let out = match &path {
        Some(path) => sh::run_with_env(root, "git", &args, &[("PATH", path.as_str())]),
        None => sh::run(root, "git", &args),
    }
    .map_err(|e| HookError::Shook(format!("could not run git: {e}")))?;
    if !out.ok() {
        let said = out.stderr.trim();
        return Err(HookError::Shook(if said.contains("not a git command") {
            "git-shook is not installed: `git shook` is not a git command here. Install it from \
             https://github.com/orgrinrt/git-shook, or put git-shook on PATH"
                .to_string()
        } else {
            format!("`git shook install --yes` refused: {said}")
        }));
    }

    let mut written = Vec::new();
    for event in hooks.events() {
        let entry = entry_path(root, event)?;
        if !entry.is_file() {
            return Err(HookError::Shook(format!(
                "`git shook install` ran and registered nothing for {event}; the manifest is at \
                 {}",
                dir.join(format!("{NAME}.toml")).display()
            )));
        }
        written.push(Written {
            event: event.to_string(),
            reach: Reach::Shook(entry.clone()),
            path:  entry,
        });
    }
    // An entrypoint this tool wrote into git's own hooks directory before the
    // repository moved to git-shook is reached by nothing there, and by a chain
    // from whatever else still runs that directory (mockspace's gate), which
    // would run the table a second time beside the entry. Removed when it is
    // exactly the text this tool writes; anything else at that path is somebody's.
    let own = git_hooks_dir(root)?;
    for event in hooks.events() {
        let stale = own.join(event);
        if std::fs::read(&stale).is_ok_and(|bytes| bytes == script(event).as_bytes()) {
            std::fs::remove_file(&stale)?;
        }
        // And the copy git-shook made of it when it adopted the hook: its
        // `00-existing` entry runs the table beside the registered one. Both
        // go when the kept file is exactly that text, and neither otherwise.
        let registry = common_dir(root)?.join("shook");
        let kept = registry.join("kept").join(event);
        if std::fs::read(&kept).is_ok_and(|bytes| bytes == script(event).as_bytes()) {
            std::fs::remove_file(&kept)?;
            let adopted = registry.join("entries").join(event).join("00-existing");
            if adopted.is_file() {
                std::fs::remove_file(&adopted)?;
            }
        }
    }
    Ok(Installed {
        written,
    })
}

#[cfg(test)]
mod tests;
