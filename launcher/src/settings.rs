//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The person's settings: the four keys the launcher declares, and the typed
//! reading of them a command does.
//!
//! All four are user scope. The manifest at a workspace root says everything
//! about that workspace; these say things about the person running the
//! launcher, which no workspace can know, and `homma workspace` is what reads
//! them, before there is a workspace at all.

use std::path::{Path, PathBuf};

use renki::Invocation;
use renki::config::Toml;
use renki_config::{Declared, List, PathText, Setting, Text, TextItems, User};

/// Where a workspace may never be made. A plain entry denies exactly that
/// directory; one ending in `/*` denies everything under it.
pub const DISALLOWED_ROOTS: &str = "disallowed_roots";
/// Where `homma workspace spawn <slug>` puts one.
pub const WORKSPACES_ROOT: &str = "workspaces_root";
/// The git url a fresh workspace is a clone of.
pub const CONTENT_REPO: &str = "spawn.content_repo";
/// What every fresh workspace clones beside the content repository.
pub const SPAWN_REPOS: &str = "spawn.repos";

/// The table, in the order `homma config schema` prints it.
pub const SETTINGS: &[Declared<Toml>] = &[
    Setting::<List<PathText>, User>::new(
        DISALLOWED_ROOTS,
        "[\"~\"]",
        "Directories a workspace may never be made in; a trailing /* denies the whole subtree.",
    )
    .row(),
    Setting::<PathText, User>::new(
        WORKSPACES_ROOT,
        "~/workspaces",
        "Where `homma workspace spawn <slug>` puts a new workspace.",
    )
    .row(),
    Setting::<Text, User>::new(
        CONTENT_REPO,
        "",
        "The git url a fresh workspace is a clone of; empty refuses to spawn.",
    )
    .row(),
    Setting::<List<Text>, User>::new(
        SPAWN_REPOS,
        "[]",
        "Repositories every fresh workspace clones beside the content one, as owner/name or a url.",
    )
    .row(),
];

/// The four, read off an invocation and typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// The entries of `disallowed_roots`, as written, `~` and all.
    pub disallowed_roots: Vec<String>,
    /// `workspaces_root`, with `~` expanded.
    pub workspaces_root:  PathBuf,
    /// `spawn.content_repo`, possibly empty.
    pub content_repo:     String,
    /// `spawn.repos`, as written.
    pub repos:            Vec<String>,
}

impl Prefs {
    /// Read the four off `inv`, expanding `~` against `home`.
    ///
    /// Every key resolves, the default at least, so a missing one is the
    /// descriptor and the table disagreeing, which the launcher refused to
    /// start on. It is still an error here rather than a panic, because a
    /// message naming the key beats a backtrace.
    pub fn read(inv: &Invocation<'_>, home: Option<&Path>) -> Result<Prefs, String> {
        let text = |key: &str| {
            inv.setting(key)
                .ok_or_else(|| format!("the launcher declares no setting {key:?}, and it should"))
        };
        let list = |key: &str| -> Result<Vec<String>, String> {
            let t = text(key)?;
            match TextItems::over(t) {
                notko::Outcome::Ok(items) => Ok(items.map(str::to_owned).collect()),
                notko::Outcome::Err(()) => Err(format!("{key} is not a list: {t:?}")),
            }
        };
        let workspaces_root = expand_home(text(WORKSPACES_ROOT)?, home)?;
        // Relative, and the destination would land wherever the shell
        // happened to be, with the denied-root check resolving against that
        // cwd rather than against anything the person named.
        if !workspaces_root.is_absolute() {
            return Err(format!(
                "{WORKSPACES_ROOT} is {:?}, which is relative; it has to be an absolute path \
                 or start with ~",
                text(WORKSPACES_ROOT)?
            ));
        }
        Ok(Prefs {
            disallowed_roots: list(DISALLOWED_ROOTS)?,
            workspaces_root,
            content_repo: text(CONTENT_REPO)?.trim().to_owned(),
            repos: list(SPAWN_REPOS)?,
        })
    }

    /// Why `dest` may not hold a workspace, or nothing.
    ///
    /// Both sides are resolved through the filesystem where they exist, so a
    /// symlink to the home directory is the home directory. A destination that
    /// does not exist yet is resolved through its nearest existing ancestor,
    /// which is what a fresh `spawn <slug>` has.
    pub fn refusal_for(&self, dest: &Path, home: Option<&Path>) -> Result<Option<String>, String> {
        let dest = resolve_through_ancestors(dest);
        for entry in &self.disallowed_roots {
            let (spelled, subtree) = match entry.strip_suffix("/*") {
                Some(s) => (s, true),
                None => (entry.as_str(), false),
            };
            let denied = expand_home(spelled, home)?;
            let denied = resolve_through_ancestors(&denied);
            let hit = if subtree { dest.starts_with(&denied) } else { dest == denied };
            if hit {
                return Ok(Some(format!(
                    "{} is under {entry:?}, which {DISALLOWED_ROOTS} in your settings says a \
                     workspace may never be made in; `homma config get {DISALLOWED_ROOTS}` shows \
                     the list",
                    dest.display()
                )));
            }
        }
        Ok(None)
    }
}

/// `~` or `~/...` against `home`, anything else as it is.
///
/// A bare `~` with no home to expand it against is an error naming the
/// setting's shape rather than a path that starts with a literal tilde, since
/// the second is a directory nobody meant.
pub fn expand_home(text: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let rest = if text == "~" { Some("") } else { text.strip_prefix("~/") };
    match rest {
        Some(rest) => {
            let home = home.ok_or_else(|| {
                format!("{text:?} starts with ~ and HOME is not set, so there is nothing to expand it against")
            })?;
            Ok(if rest.is_empty() { home.to_path_buf() } else { home.join(rest) })
        },
        None => Ok(PathBuf::from(text)),
    }
}

/// `canonicalize`, or the canonical form of the nearest existing ancestor with
/// the rest joined back on, so a path that is not there yet still resolves the
/// part of it that is.
fn resolve_through_ancestors(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    // Not there, so `.` and `..` are folded by hand first: `file_name` has
    // no answer for a trailing `..`, and a destination spelled `x/..` is the
    // directory above `x` whether or not `x` exists yet.
    let lexical = lexical_of(path);
    if let Ok(p) = lexical.canonicalize() {
        return p;
    }
    let mut tail = Vec::new();
    let mut cur = lexical;
    while let Some(name) = cur.file_name().map(|n| n.to_os_string()) {
        cur.pop();
        tail.push(name);
        if let Ok(base) = cur.canonicalize() {
            let mut out = base;
            for name in tail.iter().rev() {
                out.push(name);
            }
            return out;
        }
        if cur.as_os_str().is_empty() {
            break;
        }
    }
    // Nothing on the way exists, so the folded form is the best there is:
    // `.` and `..` still mean what they mean whether or not the rest does.
    lexical_of(path)
}

fn lexical_of(path: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => {
                lexical.pop();
            },
            other => lexical.push(other),
        }
    }
    lexical
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_usable() {
        // What the launcher checks at start, checked here so a bad default
        // fails the suite rather than the first run.
        assert!(matches!(Declared::defect(SETTINGS), notko::Maybe::Isnt));
    }

    #[test]
    fn a_tilde_expands_against_the_home_and_only_at_the_front() {
        let h = Path::new("/h");
        assert_eq!(expand_home("~", Some(h)).unwrap(), PathBuf::from("/h"));
        assert_eq!(
            expand_home("~/a/b", Some(h)).unwrap(),
            PathBuf::from("/h/a/b")
        );
        assert_eq!(expand_home("/x/~", Some(h)).unwrap(), PathBuf::from("/x/~"));
        assert_eq!(expand_home("~x", Some(h)).unwrap(), PathBuf::from("~x"));
        assert!(expand_home("~", None).is_err());
        assert!(expand_home("~/a", None).is_err());
        assert_eq!(expand_home("/x", None).unwrap(), PathBuf::from("/x"));
    }

    #[test]
    fn a_path_that_is_not_there_yet_resolves_its_existing_part() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().canonicalize().unwrap();
        let fresh = d.path().join("a").join("b");
        assert_eq!(resolve_through_ancestors(&fresh), real.join("a").join("b"));
        // and one that is there resolves whole, through a link
        std::fs::create_dir(d.path().join("t")).unwrap();
        std::os::unix::fs::symlink(d.path().join("t"), d.path().join("l")).unwrap();
        assert_eq!(
            resolve_through_ancestors(&d.path().join("l")),
            real.join("t")
        );
        // a `..` inside a path that exists nowhere is still folded, which is
        // the give-up branch, and the control is that the fold happened at
        // all rather than the path coming back as written
        let nowhere = Path::new("/nowhere-at-all/x/y/../z");
        assert_eq!(
            resolve_through_ancestors(nowhere),
            PathBuf::from("/nowhere-at-all/x/z")
        );
        assert_ne!(resolve_through_ancestors(nowhere), nowhere);
        // and the relative form with no existing ancestor at all, which is
        // the only way to reach the give-up branch on a unix where `/` exists
        let relative = Path::new("no-such-dir-here/../zed");
        assert_eq!(resolve_through_ancestors(relative), PathBuf::from("zed"));
    }
}
