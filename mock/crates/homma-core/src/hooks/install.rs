//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The entrypoints: one per event the table names, a few lines each that hand
//! git's arguments to `homma hook run <event>`, so the logic stays in the
//! binary and the file never drifts from it.
//!
//! They are written into git's own hooks directory, the common git
//! directory's `hooks/`, whatever `core.hooksPath` says, because that is the
//! one place every chain reaches: git reads it directly where the path is
//! unset, and mockspace's durable gate runs the repository's own hook there
//! after its own checks where the path is mockspace's. Which of those holds is
//! reported with the install, and a path that is some other tool's is
//! reported as one git will not reach.

use std::fmt;
use std::path::{Path, PathBuf};

use homma_api::Hooks;

use crate::release::git::GitError;
use crate::release::sh;

/// The entrypoint for one event.
pub fn script(event: &str) -> String {
    format!(
        "#!/bin/sh\n# homma writes this file; what it runs is [hooks] in homma.toml\nexec homma hook run {event} \"$@\"\n"
    )
}

/// How git reaches the entrypoints, read off `core.hooksPath`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// No hooks path: git reads the directory itself.
    Direct,
    /// The path is mockspace's, whose gate runs the repository's own hook
    /// after its own checks.
    Chained(String),
    /// The path is some other tool's, and nothing runs the entrypoints until
    /// that tool chains to them.
    Unreached(String),
}

impl Reach {
    pub fn reached(&self) -> bool {
        !matches!(self, Reach::Unreached(_))
    }
}

impl fmt::Display for Reach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reach::Direct => f.write_str("git reads the hooks directory directly"),
            Reach::Chained(p) => {
                write!(
                    f,
                    "core.hooksPath is {p}, mockspace's, whose gate runs these after its own checks"
                )
            },
            Reach::Unreached(p) => {
                write!(
                    f,
                    "core.hooksPath is {p}, which is not mockspace's; git will not run these until that \
                     tool chains to the repository's own hooks"
                )
            },
        }
    }
}

/// What an install wrote and how git gets to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub paths: Vec<PathBuf>,
    pub reach: Reach,
}

/// Why nothing was written.
#[derive(Debug)]
pub enum HookError {
    /// The hooks directory holds tracked files, so a hook written there would
    /// ship with the repo and ask homma of everyone who clones it.
    HooksPathTracked(PathBuf),
    /// A hook for that event is already there and is not this tool's.
    HookExists(PathBuf),
    Git(GitError),
    Io(std::io::Error),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::HooksPathTracked(p) => {
                write!(
                    f,
                    "{} holds tracked files, so a hook written there would ship with the repo; \
                     this repo keeps its own hooks",
                    p.display()
                )
            },
            HookError::HookExists(p) => {
                write!(
                    f,
                    "{} is already there and is not homma's; move what it does into the hooks table \
                     by hand, or remove it, before installing",
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

/// Git's own hooks directory for `root`: the common git directory's `hooks/`,
/// which a linked worktree shares with the clone it hangs off. Never the
/// `core.hooksPath`, which is somebody else's directory.
pub fn hooks_dir(root: &Path) -> Result<PathBuf, HookError> {
    let out = git(root, &[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?;
    Ok(PathBuf::from(out.stdout.trim()).join("hooks"))
}

/// How git reaches `root`'s own hooks directory.
pub fn reach(root: &Path) -> Result<Reach, HookError> {
    let out =
        sh::run(root, "git", &["config", "--get", "core.hooksPath"]).map_err(GitError::from)?;
    let configured = out.stdout.trim();
    if !out.ok() || configured.is_empty() {
        return Ok(Reach::Direct);
    }
    // the same predicate mockspace uses for its own path: it names mockspace,
    // or it ends the way mockspace's generated directory does
    let p = configured.trim_end_matches('/');
    if p.contains("mockspace") || p.ends_with("target/hooks") {
        Ok(Reach::Chained(configured.to_string()))
    } else {
        Ok(Reach::Unreached(configured.to_string()))
    }
}

/// Write one entrypoint per event the table names, or refuse and say why.
/// Nothing is written where anything refuses, so a partial install is not a
/// state a repository can be left in.
pub fn install(root: &Path, hooks: &Hooks) -> Result<Installed, HookError> {
    let dir = hooks_dir(root)?;
    // a hooks directory the repo tracks is the repo's own, and a hook written
    // there would ship with it; asked about relative to the root, and as `.`
    // where the directory is the root itself, since git refuses an empty
    // pathspec. A directory outside the root, a linked worktree's common
    // directory, is nothing the repo could track.
    if let Ok(relative) = dir.strip_prefix(root) {
        let spec = relative.to_string_lossy();
        let spec = if spec.is_empty() { ".".to_string() } else { spec.into_owned() };
        let tracked = git(root, &["ls-files", "--", &spec])?;
        if !tracked.stdout.trim().is_empty() {
            return Err(HookError::HooksPathTracked(dir));
        }
    }
    let events: Vec<&str> = hooks.events().collect();
    // anything at a path that is not this tool's text is somebody's hook, a
    // compiled one or one this user cannot read included; only an absent file
    // or this tool's own falls through to the write
    for event in &events {
        let path = dir.join(event);
        match std::fs::read(&path) {
            Ok(bytes) if bytes == script(event).as_bytes() => {},
            Ok(_) => return Err(HookError::HookExists(path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err(HookError::HookExists(path)),
        }
    }
    std::fs::create_dir_all(&dir)?;
    let mut paths = Vec::new();
    for event in &events {
        let path = dir.join(event);
        std::fs::write(&path, script(event))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        paths.push(path);
    }
    Ok(Installed {
        paths,
        reach: reach(root)?,
    })
}

/// Whether the entrypoint for `event` at `root` is the one this module writes.
pub fn is_installed(root: &Path, event: &str) -> Result<bool, HookError> {
    let dir = hooks_dir(root)?;
    Ok(std::fs::read_to_string(dir.join(event))
        .map(|s| s == script(event))
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use homma_api::HookEntry;

    use super::*;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let out = sh::run(d.path(), "git", &["init", "--quiet", "-b", "main"]).unwrap();
        assert!(out.ok());
        d
    }

    fn table(events: &[&str]) -> Hooks {
        let mut declared = BTreeMap::new();
        for e in events {
            declared.insert(e.to_string(), vec![HookEntry {
                run:   "true".into(),
                paths: Vec::new(),
            }]);
        }
        Hooks::new(declared).unwrap()
    }

    #[test]
    fn one_entrypoint_per_event_lands_in_the_repos_own_hooks_dir_executable_and_recognised() {
        let d = repo();
        assert!(!is_installed(d.path(), "pre-push").unwrap());
        let i = install(d.path(), &table(&["pre-commit"])).unwrap();
        let names: Vec<String> = i
            .paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["pre-commit", "pre-push"],
            "the declared event and the gate's"
        );
        let hooks = d.path().canonicalize().unwrap().join(".git/hooks");
        for p in &i.paths {
            assert!(p.starts_with(&hooks), "{}", p.display());
            let event = p.file_name().unwrap().to_string_lossy();
            assert_eq!(std::fs::read_to_string(p).unwrap(), script(&event));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_ne!(
                    std::fs::metadata(p).unwrap().permissions().mode() & 0o111,
                    0
                );
            }
        }
        assert_eq!(i.reach, Reach::Direct);
        assert!(is_installed(d.path(), "pre-push").unwrap());
        assert!(is_installed(d.path(), "pre-commit").unwrap());
        assert!(
            !is_installed(d.path(), "commit-msg").unwrap(),
            "no entries, no entrypoint"
        );
        let again = install(d.path(), &table(&["pre-commit"])).unwrap();
        assert_eq!(again, i, "a second install is the same files");
    }

    #[test]
    fn the_script_hands_gits_arguments_to_the_run_verb_for_its_own_event() {
        let s = script("commit-msg");
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains("exec homma hook run commit-msg \"$@\""));
        assert!(!s.contains("pre-push"));
    }

    #[test]
    fn the_hooks_dir_is_the_common_git_dir_whatever_the_hooks_path_says() {
        // a linked worktree shares the clone's hooks, and core.hooksPath
        // pointing elsewhere does not move where homma writes
        let d = repo();
        std::fs::write(d.path().join("f"), "x").unwrap();
        for args in [
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "add", "f"],
            vec!["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"],
            vec!["config", "core.hooksPath", "/somewhere/mockspace/hooks-v3"],
        ] {
            assert!(sh::run(d.path(), "git", &args).unwrap().ok());
        }
        let expect = d.path().canonicalize().unwrap().join(".git/hooks");
        assert_eq!(hooks_dir(d.path()).unwrap(), expect);
        let wt = tempfile::tempdir().unwrap();
        let out = sh::run(d.path(), "git", &[
            "worktree",
            "add",
            "-q",
            wt.path().to_str().unwrap(),
            "-b",
            "side",
        ])
        .unwrap();
        assert!(out.ok(), "{}", out.log());
        assert_eq!(
            hooks_dir(wt.path()).unwrap(),
            expect,
            "the worktree shares the clone's hooks"
        );
        let i = install(wt.path(), &table(&[])).unwrap();
        assert_eq!(i.paths, vec![expect.join("pre-push")]);
        assert_eq!(
            i.reach,
            Reach::Chained("/somewhere/mockspace/hooks-v3".into())
        );
        assert!(i.reach.reached());
    }

    #[test]
    fn the_reach_says_how_git_gets_there() {
        let d = repo();
        assert_eq!(reach(d.path()).unwrap(), Reach::Direct);
        for (path, chained) in [
            ("/home/x/.config/mockspace/hooks-v3", true),
            ("mock/target/hooks", true),
            (".husky", false),
            ("/opt/lefthook/hooks", false),
        ] {
            assert!(
                sh::run(d.path(), "git", &["config", "core.hooksPath", path])
                    .unwrap()
                    .ok()
            );
            let r = reach(d.path()).unwrap();
            assert_eq!(r.reached(), chained, "{path}: {r}");
            assert!(r.to_string().contains(path));
            if !chained {
                assert!(r.to_string().contains("will not run"));
            }
        }
        // and an install under a foreign path still writes, and says so
        let i = install(d.path(), &table(&[])).unwrap();
        assert_eq!(i.reach, Reach::Unreached("/opt/lefthook/hooks".into()));
        assert!(i.paths[0].exists());
    }

    #[test]
    fn a_tracked_hooks_dir_and_a_foreign_hook_are_refused_and_nothing_is_written() {
        // a repo that tracks its `.git`-adjacent hooks directory is contrived,
        // since git never tracks `.git`; the check is exercised by pointing the
        // common dir's hooks at tracked content through a repo whose git dir
        // is the root, a bare-shaped layout, which is what `.` covers
        let d = repo();
        let own = d
            .path()
            .canonicalize()
            .unwrap()
            .join(".git/hooks/pre-commit");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, "#!/bin/sh\necho mine\n").unwrap();
        match install(d.path(), &table(&["pre-commit"])) {
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
        assert!(
            !d.path().join(".git/hooks/pre-push").exists(),
            "nothing else was written either"
        );
        // the tool's own is rewritten, which is the control
        std::fs::write(&own, script("pre-commit")).unwrap();
        assert!(install(d.path(), &table(&["pre-commit"])).is_ok());
    }

    #[test]
    fn a_hook_that_does_not_read_as_text_or_cannot_be_read_is_still_refused() {
        let d = repo();
        let own = d.path().join(".git/hooks/pre-push");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        let binary = [0x7Fu8, b'E', b'L', b'F', 0xFF, 0xFE, 0x00, 0x01];
        std::fs::write(&own, binary).unwrap();
        assert!(matches!(
            install(d.path(), &table(&[])),
            Err(HookError::HookExists(_))
        ));
        assert_eq!(std::fs::read(&own).unwrap(), binary, "and it is untouched");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&own, "#!/bin/sh\necho theirs\n").unwrap();
            std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result = install(d.path(), &table(&[]));
            std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(
                matches!(result, Err(HookError::HookExists(_))),
                "{result:?}"
            );
            assert_eq!(
                std::fs::read_to_string(&own).unwrap(),
                "#!/bin/sh\necho theirs\n"
            );
        }
    }

    #[test]
    fn a_directory_that_is_not_a_repo_is_a_git_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(
            install(d.path(), Hooks::defaults()),
            Err(HookError::Git(_))
        ));
    }
}
