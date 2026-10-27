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
//! after its own checks where the hook git will run for that event is
//! mockspace's. Which of those holds is decided per event, off the file under
//! the hooks path rather than off the path's spelling, and reported with the
//! install; an event nothing reaches is reported as one git will not run.

use std::fmt;
use std::path::{Path, PathBuf};

use homma_api::Hooks;

pub use super::reach_kind::Reach;
use crate::release::git::GitError;
use crate::release::sh;

/// The entrypoint for one event.
pub fn script(event: &str) -> String {
    format!(
        "#!/bin/sh\n# homma writes this file; what it runs is [hooks] in homma.toml\nexec homma hook run {event} \"$@\"\n"
    )
}

/// What mockspace writes into the first lines of every hook it manages, and
/// what decides that a hook under `core.hooksPath` chains to the repository's
/// own. Spelled here rather than read from mockspace, since homma knows
/// mockspace's files and not its crates.
pub const MOCKSPACE_MARKER: &str = "# mockspace-managed";

/// One entrypoint the install wrote, and how git gets to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub event: String,
    pub path:  PathBuf,
    pub reach: Reach,
}

/// What an install wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub written: Vec<Written>,
}

impl Installed {
    /// Whether git reaches every entrypoint written.
    pub fn reached(&self) -> bool {
        self.written.iter().all(|w| w.reach.reached())
    }
}

/// Why nothing was written.
#[derive(Debug)]
pub enum HookError {
    /// A hook for that event is already there and is not this tool's.
    HookExists(PathBuf),
    /// Registering through git-shook did not go: it is not installed, it
    /// refused, or it registered less than the manifest declares. The text
    /// says which.
    Shook(String),
    Git(GitError),
    Io(std::io::Error),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::HookExists(p) => {
                write!(
                    f,
                    "{} is already there and is not homma's; move what it does into the hooks table \
                     by hand, or remove it, before installing",
                    p.display()
                )
            },
            HookError::Shook(why) => write!(f, "{why}"),
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

/// The `core.hooksPath` in effect at `root`, made absolute against it, or
/// `None` where git reads its own directory.
pub fn hooks_path(root: &Path) -> Result<Option<PathBuf>, HookError> {
    let out =
        sh::run(root, "git", &["config", "--get", "core.hooksPath"]).map_err(GitError::from)?;
    let configured = out.stdout.trim();
    if !out.ok() || configured.is_empty() {
        return Ok(None);
    }
    let p = PathBuf::from(configured);
    Ok(Some(if p.is_absolute() { p } else { root.join(p) }))
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

/// What git's own hooks directory holds for `event`: `Direct` where the file is
/// exactly the text homma writes, which is what git runs with no hooks path and
/// what mockspace's gate chains to, `NotInstalled` where there is none, and
/// `Other` where it is somebody else's.
fn own_entrypoint(root: &Path, event: &str) -> Result<Reach, HookError> {
    let file = hooks_dir(root)?.join(event);
    Ok(match std::fs::read(&file) {
        Ok(bytes) if bytes == script(event).as_bytes() => Reach::Direct,
        Ok(_) => Reach::Other(file),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Reach::NotInstalled(file),
        Err(_) => Reach::Other(file),
    })
}

/// How git reaches `root`'s own entrypoint for `event`: directly with no
/// hooks path, through git-shook's dispatcher, where homma is reached as a
/// registered entry or not at all, else through whatever sits at
/// `<hooksPath>/<event>`, which is mockspace's and chains where its first
/// lines say so.
pub fn reach(root: &Path, event: &str) -> Result<Reach, HookError> {
    let Some(dir) = hooks_path(root)? else {
        // git reads its own directory, and reaches the entrypoint only if one is
        // there: counting it reached from the absence of a hooks path alone made
        // `hook doctor` pass in a repository where nothing was ever installed
        return own_entrypoint(root, event);
    };
    if dir == super::shook::hooks_dir(root)? {
        return super::shook::reach(root, event);
    }
    let file = dir.join(event);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let head: String =
                String::from_utf8_lossy(&bytes[.. bytes.len().min(512)]).into_owned();
            if head.lines().take(4).any(|l| l.contains(MOCKSPACE_MARKER)) {
                // the gate chains to git's own directory and to nothing else,
                // so it reaches the entrypoint only where that holds it
                match own_entrypoint(root, event)? {
                    Reach::Direct => Ok(Reach::Chained(file)),
                    unreached => Ok(unreached),
                }
            } else {
                Ok(Reach::Foreign(file))
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Reach::Missing(file)),
        // a file this user cannot read is somebody's and not mockspace's
        Err(_) => Ok(Reach::Foreign(file)),
    }
}

/// Install the table: registered with git-shook where `core.hooksPath` is its
/// dispatcher directory, else one entrypoint per event. Nothing forces the
/// first: moving `core.hooksPath` onto git-shook's directory from a repository
/// where another tool holds it is that tool's key to give up and not this one's.
pub fn install(root: &Path, hooks: &Hooks) -> Result<Installed, HookError> {
    install_on(root, hooks, None)
}

/// [`install`], with `on_path` at the front of the `PATH` git-shook is looked
/// up on where it is not on the caller's. The routing is here so a test can
/// reach it with a stand-in for git-shook.
pub(crate) fn install_on(
    root: &Path,
    hooks: &Hooks,
    on_path: Option<&Path>,
) -> Result<Installed, HookError> {
    let shook_path = hooks_path(root)?.is_some_and(|p| {
        super::shook::hooks_dir(root)
            .map(|d| d == p)
            .unwrap_or(false)
    });
    if shook_path {
        return super::shook::install_with(root, hooks, on_path);
    }
    install_entrypoints(root, hooks)
}

/// Write one entrypoint per event the table names, or refuse and say why.
/// Every event is checked before any is written, so no refusal leaves a
/// partial install.
fn install_entrypoints(root: &Path, hooks: &Hooks) -> Result<Installed, HookError> {
    let dir = hooks_dir(root)?;
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
    let mut written = Vec::new();
    for event in &events {
        let path = dir.join(event);
        std::fs::write(&path, script(event))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        written.push(Written {
            event: (*event).to_string(),
            path,
            reach: reach(root, event)?,
        });
    }
    Ok(Installed {
        written,
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
            declared.insert(e.to_string(), vec![
                HookEntry::new("true", Vec::new()).unwrap(),
            ]);
        }
        Hooks::new(declared).unwrap()
    }

    fn set_hooks_path(d: &Path, value: &str) {
        assert!(
            sh::run(d, "git", &["config", "core.hooksPath", value])
                .unwrap()
                .ok()
        );
    }

    /// A directory standing in for mockspace's durable one: a managed hook
    /// for each event in `managed`, and a foreign one for each in `foreign`.
    fn hooks_path_dir(managed: &[&str], foreign: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for e in managed {
            std::fs::write(
                d.path().join(e),
                format!("#!/usr/bin/env bash\n{MOCKSPACE_MARKER} v3 fp:0000\n# mockspace durable gate ({e})\nexit 0\n"),
            )
            .unwrap();
        }
        for e in foreign {
            std::fs::write(d.path().join(e), "#!/bin/sh\necho theirs\n").unwrap();
        }
        d
    }

    #[test]
    fn one_entrypoint_per_event_lands_in_the_repos_own_hooks_dir_executable_and_recognised() {
        let d = repo();
        assert!(!is_installed(d.path(), "pre-push").unwrap());
        let i = install(d.path(), &table(&["pre-commit"])).unwrap();
        let events: Vec<&str> = i.written.iter().map(|w| w.event.as_str()).collect();
        assert_eq!(
            events,
            vec!["pre-commit", "pre-push"],
            "the declared event and the gate's"
        );
        let hooks = d.path().canonicalize().unwrap().join(".git/hooks");
        for w in &i.written {
            assert_eq!(w.path, hooks.join(&w.event));
            assert_eq!(std::fs::read_to_string(&w.path).unwrap(), script(&w.event));
            assert_eq!(w.reach, Reach::Direct);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_ne!(
                    std::fs::metadata(&w.path).unwrap().permissions().mode() & 0o111,
                    0
                );
            }
        }
        assert!(i.reached());
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
        for args in [vec!["-c", "user.name=t", "-c", "user.email=t@t", "add", "f"], vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "one",
        ]] {
            assert!(sh::run(d.path(), "git", &args).unwrap().ok());
        }
        let ms = hooks_path_dir(&["pre-commit", "pre-push", "commit-msg"], &[]);
        set_hooks_path(d.path(), ms.path().to_str().unwrap());
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
        assert_eq!(i.written.len(), 1);
        assert_eq!(i.written[0].path, expect.join("pre-push"));
        assert_eq!(
            i.written[0].reach,
            Reach::Chained(ms.path().join("pre-push"))
        );
        assert!(i.reached());
    }

    #[test]
    fn the_reach_is_per_event_and_read_off_the_hook_under_the_path() {
        let d = repo();
        // no hooks path is not the same as reached: until an entrypoint is in
        // git's own directory nothing runs, and `doctor` has to fail on it
        let missing = hooks_dir(d.path()).unwrap().join("pre-push");
        let r = reach(d.path(), "pre-push").unwrap();
        assert_eq!(r, Reach::NotInstalled(missing.clone()));
        assert!(!r.reached());
        assert!(r.to_string().contains("homma hook install"));
        std::fs::create_dir_all(missing.parent().unwrap()).unwrap();
        std::fs::write(&missing, script("pre-push")).unwrap();
        assert_eq!(reach(d.path(), "pre-push").unwrap(), Reach::Direct);
        assert_eq!(hooks_path(d.path()).unwrap(), None);
        // somebody else's file in git's own directory is what git runs, so the
        // entrypoint is not reached and `install` refusing it agrees
        let theirs = hooks_dir(d.path()).unwrap().join("pre-commit");
        std::fs::write(&theirs, "#!/bin/sh\nexit 0\n").unwrap();
        let r = reach(d.path(), "pre-commit").unwrap();
        assert_eq!(r, Reach::Other(theirs.clone()));
        assert!(!r.reached());
        assert!(r.to_string().contains("somebody else's hook"));
        std::fs::remove_file(&theirs).unwrap();
        // mockspace's three, and nothing for a fourth event
        let ms = hooks_path_dir(&["pre-commit", "pre-push", "commit-msg"], &[]);
        set_hooks_path(d.path(), ms.path().to_str().unwrap());
        assert_eq!(
            reach(d.path(), "pre-push").unwrap(),
            Reach::Chained(ms.path().join("pre-push"))
        );
        // mockspace's gate chains to git's own directory and nowhere else: with
        // no entrypoint there, or somebody else's, the table never runs
        let none = hooks_dir(d.path()).unwrap().join("pre-commit");
        let r = reach(d.path(), "pre-commit").unwrap();
        assert_eq!(r, Reach::NotInstalled(none.clone()));
        assert!(!r.reached());
        std::fs::write(&none, "#!/bin/sh\nexit 0\n").unwrap();
        assert_eq!(
            reach(d.path(), "pre-commit").unwrap(),
            Reach::Other(none.clone())
        );
        std::fs::remove_file(&none).unwrap();
        assert_eq!(
            reach(d.path(), "post-merge").unwrap(),
            Reach::Missing(ms.path().join("post-merge"))
        );
        assert!(!reach(d.path(), "post-merge").unwrap().reached());
        assert!(
            reach(d.path(), "post-merge")
                .unwrap()
                .to_string()
                .contains("nothing at")
        );
        // a path spelled like mockspace's with nothing in it reaches nothing;
        // the spelling was what the first version read, and it was wrong
        let empty = tempfile::tempdir().unwrap();
        let spelled = empty.path().join("mockspace").join("hooks-v3");
        std::fs::create_dir_all(&spelled).unwrap();
        set_hooks_path(d.path(), spelled.to_str().unwrap());
        assert!(matches!(
            reach(d.path(), "pre-push").unwrap(),
            Reach::Missing(_)
        ));
        // another tool's hook under the path is foreign, and says so
        let other = hooks_path_dir(&[], &["pre-push"]);
        set_hooks_path(d.path(), other.path().to_str().unwrap());
        let r = reach(d.path(), "pre-push").unwrap();
        assert_eq!(r, Reach::Foreign(other.path().join("pre-push")));
        assert!(r.to_string().contains("not mockspace's"));
        // a relative path is against the root
        std::fs::create_dir_all(d.path().join(".githooks")).unwrap();
        std::fs::write(
            d.path().join(".githooks/pre-push"),
            format!("#!/bin/sh\n{MOCKSPACE_MARKER}\n"),
        )
        .unwrap();
        set_hooks_path(d.path(), ".githooks");
        assert_eq!(
            hooks_path(d.path()).unwrap(),
            Some(d.path().join(".githooks"))
        );
        assert!(matches!(
            reach(d.path(), "pre-push").unwrap(),
            Reach::Chained(_)
        ));
        // an install under a foreign path still writes, and the install says
        // it is not reached
        set_hooks_path(d.path(), other.path().to_str().unwrap());
        let i = install(d.path(), &table(&["pre-commit"])).unwrap();
        assert!(!i.reached());
        assert!(i.written.iter().all(|w| w.path.exists()));
        let by_event: BTreeMap<&str, &Reach> = i
            .written
            .iter()
            .map(|w| (w.event.as_str(), &w.reach))
            .collect();
        assert!(matches!(by_event["pre-push"], Reach::Foreign(_)));
        assert!(matches!(by_event["pre-commit"], Reach::Missing(_)));
    }

    #[test]
    fn git_shooks_hooks_path_is_its_own_kind_and_reached_only_once_registered() {
        // git-shook's dispatcher runs what is registered with it and chains to
        // nothing, so `Foreign`'s advice, that git will run the entrypoint
        // once that tool chains to it, is advice about something that is not
        // going to happen. What a reader can act on is the registration, so
        // this is a kind of its own and says so.
        let d = repo();
        let shook = super::super::shook::hooks_dir(d.path()).unwrap();
        std::fs::create_dir_all(&shook).unwrap();
        set_hooks_path(d.path(), shook.to_str().unwrap());

        let r = reach(d.path(), "pre-push").unwrap();
        assert_eq!(r, Reach::ShookUnregistered(shook.clone()));
        assert!(!r.reached());
        assert!(r.to_string().contains("homma hook install"));
        assert!(!r.to_string().contains("not mockspace's"));

        // and an empty dispatcher directory is still git-shook's: the kind is
        // decided by the path being the one that tool generates, not by what
        // it holds, since the entries live beside it rather than under it
        assert!(matches!(
            reach(d.path(), "pre-commit").unwrap(),
            Reach::ShookUnregistered(_)
        ));

        // homma's entry registered for one event reaches that event and no
        // other, which is the control for the two above
        let entry = super::super::shook::entry_path(d.path(), "pre-push").unwrap();
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, "\nhomma\nhook\nrun\npre-push\n").unwrap();
        let r = reach(d.path(), "pre-push").unwrap();
        assert_eq!(r, Reach::Shook(entry));
        assert!(r.reached());
        assert!(matches!(
            reach(d.path(), "pre-commit").unwrap(),
            Reach::ShookUnregistered(_)
        ));

        // a sibling directory of the git directory spelled the same way is
        // not it, which is what comparing rather than matching a spelling buys
        let elsewhere = tempfile::tempdir().unwrap();
        let lookalike = elsewhere.path().join("shook").join("hooks");
        std::fs::create_dir_all(&lookalike).unwrap();
        set_hooks_path(d.path(), lookalike.to_str().unwrap());
        assert!(matches!(
            reach(d.path(), "pre-push").unwrap(),
            Reach::Missing(_)
        ));
    }

    #[test]
    fn a_foreign_hook_is_refused_and_nothing_is_written() {
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
            !own.parent().unwrap().join("pre-push").exists(),
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
