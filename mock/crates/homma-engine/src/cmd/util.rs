//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Cross-command helpers.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use homma_api::AbsPath;
use homma_core::RepoConfig;
use homma_core::config::Config;

/// Resolve a repo-relative path against the workspace root.
///
/// **A second, untyped copy of [`AbsPath::resolve`] stood here** with six
/// callers, written before the type existed and left standing when it arrived.
/// It now delegates, so `..` is normalised the same way everywhere and one
/// definition governs.
///
/// Returns a `PathBuf` because these callers do not take `AbsPath`, and
/// converting them is work for the round that needs it rather than this one.
/// The resolution itself is no longer duplicated, which is the part that
/// mattered: two spellings of it disagreed about `..`.
pub(crate) fn resolve_local_path(workspace_root: &Path, repo_path: &Path) -> PathBuf {
    match AbsPath::new(workspace_root) {
        Ok(root) => AbsPath::resolve(&root, repo_path).into_path_buf(),
        // A relative workspace root is not something this can fix, and
        // inventing a base would hide it. The old shape, unchanged.
        Err(_) => {
            if repo_path.is_absolute() {
                repo_path.to_path_buf()
            } else {
                workspace_root.join(repo_path)
            }
        },
    }
}

/// The error for a repository name that names no member of this workspace.
///
/// One place, because the four commands that need it used to carry four copies
/// of a sentence telling the operator to declare the repo in a table the
/// manifest no longer accepts. Membership is read off the directories under the
/// workspace root, so the honest answer names what was found rather than what
/// the operator should have written.
pub(crate) fn no_such_member(cfg: &Config, name: &str) -> anyhow::Error {
    // a clone the scope leaves out is not a member, and saying "no such repository" of one that
    // is on disk would send the operator looking for the wrong thing
    if !cfg.scope.admits(name) {
        return anyhow::anyhow!(
            "`{name}` is outside this clone's scope (`repos` under `[instance]` in \
             homma.local.toml)"
        );
    }
    let mut found: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
    found.sort_unstable();
    if found.is_empty() {
        anyhow::anyhow!(
            "no repository named `{name}` under {}, and no repository at all was found there",
            cfg.workspace.path.display()
        )
    } else {
        anyhow::anyhow!(
            "no repository named `{name}` under {}. found: {}",
            cfg.workspace.path.display(),
            found.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_outside_the_scope_is_said_to_be_outside_it_and_a_stranger_is_not() {
        let mut cfg = Config::parse("[workspace]\nname = \"w\"\n").unwrap();
        cfg.scope = homma_core::scope::Scope::named(["kaski"]);
        let outside = no_such_member(&cfg, "arvo").to_string();
        assert!(outside.contains("outside this clone's scope"), "{outside}");
        cfg.scope = homma_core::scope::Scope::everything();
        let stranger = no_such_member(&cfg, "arvo").to_string();
        assert!(
            stranger.contains("no repository named `arvo`"),
            "{stranger}"
        );
    }

    #[test]
    fn a_relative_path_anchors_at_the_root() {
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("notko")),
            PathBuf::from("/srv/ws/notko")
        );
    }

    #[test]
    fn an_absolute_path_passes_through() {
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("/elsewhere/notko")),
            PathBuf::from("/elsewhere/notko")
        );
    }

    #[test]
    fn a_parent_component_is_normalised_the_same_way_the_type_does_it() {
        // The reason this delegates. Two spellings of one resolution disagreed
        // about `..`, and a containment check walking the un-normalised one
        // read a sibling as nested.
        assert_eq!(
            resolve_local_path(Path::new("/srv/ws"), Path::new("../out/notko")),
            PathBuf::from("/srv/out/notko")
        );
    }
}

/// Render a path with its no-op `.` components dropped, using `/` separators.
///
/// A `.` carries no information and every consumer downstream compares
/// textually: the aggregated wrapper's scope check, the workspace gate's
/// repo-table lookup, and anything else that joins the value to a root. So
/// `./arvo` joined to `/ws` gives `/ws/./arvo`, which is not a textual prefix
/// of `/ws/arvo/src/lib.rs`, and the check that was supposed to fire does not.
///
/// Normalising here rather than in the emitted shell is deliberate: the shell
/// would have to be fixed once per consumer, and each one would drift.
///
/// A path that normalises to nothing renders as `.`, because a repo whose
/// `local_path` is the workspace root is a repo at the workspace root rather
/// than a repo with no path at all.
pub(crate) fn relative_str(p: &Path) -> String {
    use std::path::Component;
    let mut out = String::new();
    for c in p.components() {
        match c {
            Component::CurDir => continue,
            Component::RootDir => {
                out.push('/');
                continue;
            },
            other => {
                if !out.is_empty() && !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str(&other.as_os_str().to_string_lossy());
            },
        }
    }
    if out.is_empty() {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod relative_str_tests {
    use super::*;

    #[test]
    fn a_curdir_component_is_dropped() {
        assert_eq!(relative_str(Path::new("./arvo")), "arvo");
        assert_eq!(relative_str(Path::new("a/./b")), "a/b");
    }

    #[test]
    fn a_path_with_nothing_to_drop_is_unchanged() {
        // The control. Without it a helper that rewrote paths generally would
        // satisfy the assertions above.
        assert_eq!(relative_str(Path::new("arvo")), "arvo");
        assert_eq!(relative_str(Path::new("a/b/c")), "a/b/c");
    }

    #[test]
    fn a_path_that_normalises_to_nothing_is_the_current_directory() {
        assert_eq!(relative_str(Path::new(".")), ".");
        assert_eq!(relative_str(Path::new("")), ".");
    }

    #[test]
    fn an_absolute_path_keeps_its_leading_separator() {
        // Both the wrapper and the gate branch on that separator to decide
        // whether to join against the workspace, so losing it silently changes
        // which arm runs.
        assert_eq!(
            relative_str(Path::new("/elsewhere/arvo")),
            "/elsewhere/arvo"
        );
        assert_eq!(relative_str(Path::new("/./a")), "/a");
    }

    #[test]
    fn a_climbing_component_is_kept_rather_than_resolved() {
        // This drops no-op components and nothing else. Collapsing `x/..`
        // would resolve a symlink the way the shell prints it rather than the
        // way the kernel walks it, which is a different decision than this
        // helper is entitled to make.
        assert_eq!(relative_str(Path::new("../arvo")), "../arvo");
    }
}

/// The clone `dir` is in, when the scope leaves that clone out of the members.
///
/// A clone directly under the workspace root whose name the scope does not admit: the case where
/// the manifest lists no clone for the working directory and refusing would lock the clone out
/// of its own git hooks. The workspace repository, a clone elsewhere, and a directory the scope
/// admits but detection missed are not it, and keep their refusal.
pub(crate) fn out_of_scope_clone(cfg: &Config, dir: &Path) -> Option<PathBuf> {
    let clone = clone_of(dir)?;
    let root = cfg.workspace.path.canonicalize().ok()?;
    let name = clone.strip_prefix(&root).ok()?;
    let mut parts = name.components();
    let first = parts.next()?;
    let direct = parts.next().is_none();
    let name = first.as_os_str().to_str()?;
    (direct && !cfg.scope.admits(name)).then_some(clone)
}

/// The clone `dir` sits in or hangs off, found through git rather than through the manifest.
///
/// A worktree sits beside the clones rather than under one, and a hook runs with it as the
/// working directory; the clone it hangs off is the parent of the common git directory. This
/// holds for a plain clone too, whose common directory is its own `.git`.
pub(crate) fn clone_of(dir: &Path) -> Option<PathBuf> {
    let common = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))?;
    let clone = common.parent()?;
    Some(clone.canonicalize().unwrap_or_else(|_| clone.to_path_buf()))
}

/// The repo named, or the one the working directory is inside, with its
/// root made absolute against the workspace.
pub(crate) fn resolve_repo<'a>(
    cfg: &'a Config,
    name: Option<&str>,
) -> Result<(&'a str, &'a RepoConfig, PathBuf)> {
    let root_of =
        |r: &RepoConfig| crate::cmd::util::resolve_local_path(&cfg.workspace.path, &r.local_path);
    if let Some(n) = name {
        return cfg
            .repos
            .get_key_value(n)
            .map(|(k, r)| (k.as_str(), r, root_of(r)))
            .ok_or_else(|| no_such_member(cfg, n));
    }
    let here = std::env::current_dir()?;
    let here = here.canonicalize().unwrap_or(here);
    let containing = |dir: &Path| {
        cfg.repos
            .iter()
            .map(|(n, r)| (n.as_str(), r, root_of(r)))
            .filter(|(_, _, root)| {
                let p = root.canonicalize().unwrap_or_else(|_| root.clone());
                dir.starts_with(&p)
            })
            .max_by_key(|(_, _, root)| root.components().count())
    };
    if let Some(found) = containing(&here) {
        return Ok(found);
    }
    if let Some(clone) = clone_of(&here) {
        if let Some(found) = containing(&clone) {
            return Ok(found);
        }
    }
    Err(anyhow!(
        "the working directory is not inside a workspace repository; name one"
    ))
}

/// The trunk and the release line for this repo: the workspace's working
/// branch onto its public one, or the public one alone where the repo has no
/// working branch, which is the shape of a crate released off `main`.
pub(crate) fn branches_for<'a>(cfg: &'a Config, root: &Path) -> (&'a str, &'a str) {
    let public = cfg.defaults.public_branch.as_str();
    let working = cfg.defaults.working_branch.as_str();
    // a fresh clone holds `origin/dev` and no local `dev`, and it still
    // releases by merging, so the remote ref counts as the branch being there
    let here = homma_core::release::git::sha(root, working).is_ok();
    let there = homma_core::release::git::sha(root, &format!("origin/{working}")).is_ok();
    if here || there { (working, public) } else { (public, public) }
}
