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
    /// The hooks directory holds tracked files, so a hook written there
    /// would ship with the repo and ask homma of everyone who clones it.
    HooksPathTracked(PathBuf),
    /// A `pre-push` is already there and is not this tool's.
    HookExists(PathBuf),
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
            HookError::HooksPathTracked(p) => {
                write!(
                    f,
                    "{} holds tracked files, so a hook written there would ship with the repo; \
                     this repo keeps its own hooks and is gated at release time",
                    p.display()
                )
            },
            HookError::HookExists(p) => {
                write!(
                    f,
                    "{} is already there and is not homma's; move the gate into it by hand, or \
                     remove it, before installing",
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
    // a hooks directory the repo tracks is the repo's own, and a hook
    // written there would ship with it; a pre-push already there that is
    // not this tool's is somebody's work and is not overwritten
    let relative = dir.strip_prefix(root).unwrap_or(&dir).to_path_buf();
    let tracked = sh::run(root, "git", &[
        "ls-files",
        "--",
        &relative.to_string_lossy(),
    ])
    .map_err(GitError::from)?;
    if !tracked.ok() {
        return Err(GitError::Failed {
            command: tracked.command_line(),
            stderr:  tracked.stderr,
        }
        .into());
    }
    if !tracked.stdout.trim().is_empty() {
        return Err(HookError::HooksPathTracked(dir));
    }
    // anything at the path that is not this tool's text is somebody's hook,
    // a compiled one or one this user cannot read included; only an absent
    // file falls through to the write
    let path = dir.join("pre-push");
    match std::fs::read(&path) {
        Ok(bytes) if bytes == SCRIPT.as_bytes() => {},
        Ok(_) => return Err(HookError::HookExists(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(_) => return Err(HookError::HookExists(path)),
    }
    std::fs::create_dir_all(&dir)?;
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
    fn a_tracked_hooks_dir_and_a_foreign_pre_push_are_refused_and_the_tools_own_is_not() {
        // a repo that ships its hooks: the directory is tracked, so a hook
        // written there would go to everyone who clones it
        let d = repo();
        std::fs::create_dir_all(d.path().join(".githooks")).unwrap();
        std::fs::write(d.path().join(".githooks/pre-commit"), "#!/bin/sh\n").unwrap();
        for args in [
            vec!["config", "core.hooksPath", ".githooks"],
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "add", ".githooks"],
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "hooks"],
        ] {
            let out = sh::run(d.path(), "git", &args).unwrap();
            assert!(out.ok(), "{}", out.log());
        }
        match install(d.path()) {
            Err(HookError::HooksPathTracked(p)) => {
                assert!(p.ends_with(".githooks"), "{}", p.display());
                assert!(
                    HookError::HooksPathTracked(p)
                        .to_string()
                        .contains("tracked")
                );
            },
            other => panic!("{other:?}"),
        }
        assert!(
            !d.path().join(".githooks/pre-push").exists(),
            "nothing was written"
        );
        // a repo with its own pre-push under .git/hooks keeps it
        let d = repo();
        let own = d.path().join(".git/hooks/pre-push");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, "#!/bin/sh\necho mine\n").unwrap();
        match install(d.path()) {
            Err(HookError::HookExists(p)) => {
                assert_eq!(p, own);
                assert!(HookError::HookExists(p).to_string().contains("not homma's"));
            },
            other => panic!("{other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(&own).unwrap(),
            "#!/bin/sh\necho mine\n"
        );
        // and the tool's own is rewritten, which is the control
        std::fs::write(&own, SCRIPT).unwrap();
        assert!(install(d.path()).is_ok());
    }

    #[test]
    fn an_untracked_hooks_dir_in_a_repo_with_tracked_files_still_takes_the_hook() {
        // the negative control on the tracked check: a repo with commits
        // whose hooks directory is not among them is a repo to install in
        let d = repo();
        std::fs::write(d.path().join("README.md"), "x\n").unwrap();
        for args in [
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "add", "README.md"],
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"],
            vec!["config", "core.hooksPath", ".githooks"],
        ] {
            let out = sh::run(d.path(), "git", &args).unwrap();
            assert!(out.ok(), "{}", out.log());
        }
        let i = install(d.path()).unwrap();
        assert!(
            i.path.ends_with(".githooks/pre-push"),
            "{}",
            i.path.display()
        );
        assert!(is_installed(d.path()).unwrap());
    }

    #[test]
    fn a_hook_that_does_not_read_as_text_or_cannot_be_read_is_still_refused() {
        // a compiled hook: bytes that are not text
        let d = repo();
        let own = d.path().join(".git/hooks/pre-push");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        let binary = [0x7Fu8, b'E', b'L', b'F', 0xFF, 0xFE, 0x00, 0x01];
        std::fs::write(&own, binary).unwrap();
        assert!(
            matches!(install(d.path()), Err(HookError::HookExists(_))),
            "a binary hook is somebody's hook"
        );
        assert_eq!(std::fs::read(&own).unwrap(), binary, "and it is untouched");
        // a hook this user cannot read
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&own, "#!/bin/sh\necho theirs\n").unwrap();
            std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result = install(d.path());
            std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                matches!(result, Err(HookError::HookExists(_))),
                "an unreadable hook is somebody's hook: {result:?}"
            );
            assert_eq!(
                std::fs::read_to_string(&own).unwrap(),
                "#!/bin/sh\necho theirs\n"
            );
        }
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
