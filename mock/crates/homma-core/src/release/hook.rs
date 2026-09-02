//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The `pre-push` hook: a few lines that hand every push to
//! `homma release gate --hook`, so the logic stays in the binary and the file
//! never drifts from it.

use std::fmt;
use std::path::{Path, PathBuf};

use super::git::GitError;
use super::sh;

/// What the hook file holds. The hook receives the refs on stdin and the
/// remote on its arguments, and the gate subcommand reads both.
pub const SCRIPT: &str = "#!/bin/sh\n# homma writes this file; the gate itself is `homma release gate --hook`\nexec homma release gate --hook \"$@\"\n";

/// Where the hook landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub path: PathBuf,
}

/// Why the hook was not written.
#[derive(Debug)]
pub enum HookError {
    /// `core.hooksPath` resolves outside the repo, which is a repo whose
    /// hooks another tool routes, mockspace in this workspace.
    HooksPathOutside(PathBuf),
    Git(GitError),
    Io(std::io::Error),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::HooksPathOutside(p) => {
                write!(
                    f,
                    "core.hooksPath is {}, outside the repo, so another tool owns the hooks here \
                     (mockspace, for a repo it manages); this repo is gated at release time until \
                     that tool's pre-push delegates to homma",
                    p.display()
                )
            },
            HookError::Git(e) => write!(f, "{e}"),
            HookError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HookError {}

impl From<GitError> for HookError {
    fn from(e: GitError) -> Self {
        HookError::Git(e)
    }
}

impl From<std::io::Error> for HookError {
    fn from(e: std::io::Error) -> Self {
        HookError::Io(e)
    }
}

/// The directory git will read hooks from for `root`, and whether it lies
/// inside the repo.
pub fn hooks_dir(root: &Path) -> Result<(PathBuf, bool), HookError> {
    let out =
        sh::run(root, "git", &["config", "--get", "core.hooksPath"]).map_err(GitError::from)?;
    let configured = out.stdout.trim();
    let dir = if out.ok() && !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.is_absolute() { p } else { root.join(p) }
    } else {
        let hooks =
            sh::run(root, "git", &["rev-parse", "--git-path", "hooks"]).map_err(GitError::from)?;
        if !hooks.ok() {
            return Err(GitError::Failed {
                command: hooks.command_line(),
                stderr:  hooks.stderr,
            }
            .into());
        }
        let p = PathBuf::from(hooks.stdout.trim());
        if p.is_absolute() { p } else { root.join(p) }
    };
    let canon_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // the directory may not exist yet, so canonicalise its parent and rejoin
    let canon_dir = match dir.canonicalize() {
        Ok(c) => c,
        Err(_) => {
            match (dir.parent(), dir.file_name()) {
                (Some(parent), Some(name)) => {
                    parent
                        .canonicalize()
                        .map(|c| c.join(name))
                        .unwrap_or_else(|_| dir.clone())
                },
                _ => dir.clone(),
            }
        },
    };
    let inside = canon_dir.starts_with(&canon_root);
    Ok((dir, inside))
}

/// Write the `pre-push` hook for `root`, or refuse and say why.
pub fn install(root: &Path) -> Result<Installed, HookError> {
    let (dir, inside) = hooks_dir(root)?;
    if !inside {
        return Err(HookError::HooksPathOutside(dir));
    }
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("pre-push");
    std::fs::write(&path, SCRIPT)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(Installed {
        path,
    })
}

/// Whether the hook at `root` is the one this module writes.
pub fn is_installed(root: &Path) -> Result<bool, HookError> {
    let (dir, _) = hooks_dir(root)?;
    Ok(std::fs::read_to_string(dir.join("pre-push"))
        .map(|s| s == SCRIPT)
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let out = sh::run(d.path(), "git", &["init", "--quiet", "-b", "main"]).unwrap();
        assert!(out.ok());
        d
    }

    #[test]
    fn the_hook_lands_in_the_repos_hooks_dir_executable_and_is_recognised() {
        let d = repo();
        assert!(!is_installed(d.path()).unwrap());
        let i = install(d.path()).unwrap();
        assert!(
            i.path.ends_with(".git/hooks/pre-push"),
            "{}",
            i.path.display()
        );
        assert_eq!(std::fs::read_to_string(&i.path).unwrap(), SCRIPT);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                std::fs::metadata(&i.path).unwrap().permissions().mode() & 0o111,
                0
            );
        }
        assert!(is_installed(d.path()).unwrap());
        let again = install(d.path()).unwrap();
        assert_eq!(again, i, "a second install is the same file");
    }

    #[test]
    fn a_hooks_path_inside_the_repo_is_honoured_and_one_outside_is_refused() {
        let d = repo();
        let out = sh::run(d.path(), "git", &["config", "core.hooksPath", ".githooks"]).unwrap();
        assert!(out.ok());
        let i = install(d.path()).unwrap();
        assert!(
            i.path.ends_with(".githooks/pre-push"),
            "{}",
            i.path.display()
        );
        let elsewhere = tempfile::tempdir().unwrap();
        let out = sh::run(d.path(), "git", &[
            "config",
            "core.hooksPath",
            elsewhere.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(out.ok());
        match install(d.path()) {
            Err(HookError::HooksPathOutside(p)) => {
                assert_eq!(p, elsewhere.path());
                assert!(
                    HookError::HooksPathOutside(p)
                        .to_string()
                        .contains("mockspace")
                );
            },
            other => panic!("{other:?}"),
        }
        assert!(
            !elsewhere.path().join("pre-push").exists(),
            "nothing was written outside"
        );
        assert!(!is_installed(d.path()).unwrap());
    }

    #[test]
    fn a_directory_that_is_not_a_repo_is_a_git_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(install(d.path()), Err(HookError::Git(_))));
    }
}
