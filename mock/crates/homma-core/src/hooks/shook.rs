//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//! Registering the git hooks with git-shook.
//!
//! Two clone-local manifests go into the common git directory's
//! `shook/manifests/`, where git-shook reads them beside the tracked
//! `shook.toml` files: homma's own, `homma`, holding the release gate, and a
//! copy of the workspace's git hooks manifest under its own name. Running
//! `homma hook install` is the consent git-shook asks for, for those two and no
//! other, so the install runs `git shook install --yes --only homma --only
//! <name>` itself. Only where git-shook holds `core.hooksPath`: homma never
//! takes that key.

use std::path::{Path, PathBuf};

use super::{HookError, legacy};
use crate::release::git::GitError;
use crate::release::sh;

/// The name homma registers its gate under, which is its manifest's name and
/// the file each entry sits in. A tracked `shook.toml` of the same name is
/// replaced by the clone-local one, so the registry holds one entry and not two.
pub const NAME: &str = "homma";

/// The event the release gate runs on.
pub const GATE_EVENT: &str = "pre-push";

/// The release gate's argument list, the `pre-push` row every workspace has
/// without writing it. git-shook appends git's arguments and replays stdin.
pub const GATE: [&str; 4] = ["homma", "release", "gate", "--hook"];

/// What the manifest homma writes says it is, at its head.
const HEADER: &str = "# homma writes this file, and `homma hook install` rewrites it: the release\n\
                      # gate every workspace pushes through. The workspace's own git hooks are a\n\
                      # manifest of their own beside this one.\n";

fn git(root: &Path, args: &[&str]) -> Result<sh::Output, HookError> {
    let out = sh::run(root, "git", args).map_err(GitError::from)?;
    if !out.ok() {
        return Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        }
        .into());
    }
    Ok(out)
}

/// The common git directory, absolute, which a linked worktree shares with the
/// clone it hangs off.
pub(super) fn common_dir(root: &Path) -> Result<PathBuf, HookError> {
    let out = git(root, &[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?;
    Ok(PathBuf::from(out.stdout.trim()))
}

/// The directory git-shook generates its dispatchers into, which is what it
/// points `core.hooksPath` at.
pub fn hooks_dir(root: &Path) -> Result<PathBuf, HookError> {
    Ok(common_dir(root)?.join("shook").join("hooks"))
}

/// Where git-shook reads the manifests this clone keeps to itself.
pub fn manifests_dir(root: &Path) -> Result<PathBuf, HookError> {
    Ok(common_dir(root)?.join("shook").join("manifests"))
}

/// Whether git-shook holds `root`'s `core.hooksPath`: the setting, made
/// absolute against `root`, is git-shook's dispatcher directory. Compared and
/// not matched by spelling, since a repository may keep a directory called
/// `shook` and mean nothing by it.
pub fn holds_path(root: &Path) -> Result<bool, HookError> {
    let out =
        sh::run(root, "git", &["config", "--get", "core.hooksPath"]).map_err(GitError::from)?;
    let configured = out.stdout.trim();
    if !out.ok() || configured.is_empty() {
        return Ok(false);
    }
    let set = PathBuf::from(configured);
    let set = if set.is_absolute() { set } else { root.join(set) };
    let ours = hooks_dir(root)?;
    Ok(set == ours
        || std::fs::canonicalize(&set)
            .ok()
            .is_some_and(|s| Some(s) == std::fs::canonicalize(&ours).ok()))
}

/// The manifest homma registers: the release gate at `pre-push`.
pub fn gate_manifest() -> String {
    let argv: Vec<String> = GATE.iter().map(|a| format!("\"{a}\"")).collect();
    format!(
        "{HEADER}name = \"{NAME}\"\n\n[hooks.{GATE_EVENT}]\nrun = [{}]\n",
        argv.join(", ")
    )
}

/// The workspace's git hooks manifest, as found at its root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceHooks {
    /// Where it is.
    pub path: PathBuf,
    /// What git-shook registers it under: its `name`, else the workspace
    /// root's directory name.
    pub name: String,
    /// The file as it is, which the copy carries byte for byte.
    pub text: String,
}

/// A name git-shook takes and a file name cannot escape with.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Read the workspace's git hooks manifest at `ws_root`/`file`, or `None`
/// where the workspace has none. Only its `name` is read: the rest is
/// git-shook's to read and refuse.
pub fn workspace_hooks(ws_root: &Path, file: &Path) -> Result<Option<WorkspaceHooks>, HookError> {
    let path = ws_root.join(file);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| {
        HookError::Manifest {
            path:   path.clone(),
            reason: e.to_string(),
        }
    })?;
    let name = match table.get("name") {
        Some(toml::Value::String(name)) => name.clone(),
        Some(_) => {
            return Err(HookError::Manifest {
                path,
                reason: "`name` is not a string".into(),
            });
        },
        None => {
            // git-shook's own rule for a manifest at a root: the directory's name
            let root = std::fs::canonicalize(ws_root).unwrap_or_else(|_| ws_root.to_path_buf());
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        },
    };
    if name == NAME || !plain_name(&name) {
        return Err(HookError::Name {
            path,
            name,
        });
    }
    Ok(Some(WorkspaceHooks {
        path,
        name,
        text,
    }))
}

/// What an install registered and took away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The manifests git-shook registered, homma's first.
    pub registered: Vec<String>,
    /// What an older homma left that the install removed.
    pub removed:    Vec<PathBuf>,
}

/// Place the manifests in `root` and have git-shook register them.
pub fn install(root: &Path, workspace: Option<&WorkspaceHooks>) -> Result<Installed, HookError> {
    install_with(root, workspace, None)
}

/// [`install`], with `on_path` put at the front of the child's `PATH` where
/// git-shook is not on the caller's.
pub fn install_with(
    root: &Path,
    workspace: Option<&WorkspaceHooks>,
    on_path: Option<&Path>,
) -> Result<Installed, HookError> {
    if !holds_path(root)? {
        return Err(HookError::NotShook(root.to_path_buf()));
    }
    let dir = manifests_dir(root)?;
    let mut files = vec![(dir.join(format!("{NAME}.toml")), gate_manifest())];
    if let Some(w) = workspace {
        files.push((dir.join(format!("{}.toml", w.name)), w.text.clone()));
    }
    let prior: Vec<Option<Vec<u8>>> = files.iter().map(|(f, _)| std::fs::read(f).ok()).collect();
    // An older homma's entrypoints, and git-shook's copies of them, go before
    // the new manifests are registered: kept, they would run beside the new
    // gate. A refusal puts them back, so a clone is never left ungated.
    let removed = legacy::clear(root)?;
    let registered = match register(root, &dir, &files, on_path) {
        Ok(names) => names,
        Err(e) => {
            // a refusal leaves each manifest as it was, not a file every later
            // install and `doctor` in the clone has to read
            for ((file, _), before) in files.iter().zip(prior) {
                let _ = match before {
                    Some(text) => std::fs::write(file, text),
                    None => std::fs::remove_file(file),
                };
            }
            legacy::restore(&removed);
            return Err(e);
        },
    };
    Ok(Installed {
        registered,
        removed: removed.into_iter().map(|r| r.path).collect(),
    })
}

fn register(
    root: &Path,
    dir: &Path,
    files: &[(PathBuf, String)],
    on_path: Option<&Path>,
) -> Result<Vec<String>, HookError> {
    std::fs::create_dir_all(dir)?;
    let mut names = Vec::new();
    for (file, text) in files {
        std::fs::write(file, text)?;
        if let Some(stem) = file.file_stem() {
            names.push(stem.to_string_lossy().into_owned());
        }
    }
    let path = on_path.map(|p| {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let mut joined = std::ffi::OsString::from(p);
        joined.push(":");
        joined.push(rest);
        joined.to_string_lossy().into_owned()
    });
    // `--only`, once per name: running this verb is the consent for these two
    // manifests, and not for a tracked `shook.toml` that arrived through a
    // merge and has not been looked at
    let mut args = vec!["shook", "install", "--yes"];
    for n in &names {
        args.push("--only");
        args.push(n);
    }
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
            format!("`git {}` refused: {said}", args.join(" "))
        }));
    }
    let gate = common_dir(root)?
        .join("shook")
        .join("entries")
        .join(GATE_EVENT)
        .join(NAME);
    if !gate.is_file() {
        return Err(HookError::Shook(format!(
            "`git shook install` ran and registered no release gate at {GATE_EVENT}; the manifest \
             is at {}",
            dir.join(format!("{NAME}.toml")).display()
        )));
    }
    Ok(names)
}

#[cfg(test)]
mod tests;
