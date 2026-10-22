//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Build directories under a workspace other than its one `target/`.
//!
//! A workspace builds into a single `target/` at its root. A second one is a
//! build that went somewhere it should not, and every worktree that grows its
//! own pays for another copy of every dependency. `homma status` lists them
//! with their sizes so nobody has to go looking.
//!
//! A directory counts as a build directory when it is called `target` and holds
//! a `CACHEDIR.TAG`, a `.rustc_info.json`, or a `debug` or `release` directory,
//! so a source directory that happens to be called `target` is not reported.
//! The walk stops six levels down, does not enter a build directory it found,
//! `.git` or `node_modules`, and follows no symlink. Nothing is deleted.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// How far below the workspace root a build directory is looked for.
const MAX_DEPTH: usize = 6;

/// Directories never entered.
const SKIPPED: [&str; 2] = [".git", "node_modules"];

/// One build directory that is not the workspace's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StrayTarget {
    pub path:  String,
    pub bytes: u64,
}

/// Every stray build directory under `root`, in path order.
pub fn collect(root: &Path) -> Vec<StrayTarget> {
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<StrayTarget>) {
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `file_type` of a symlink says symlink, so a link to a directory is
        // neither entered nor counted.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let path = entry.path();
        if SKIPPED.iter().any(|s| name == *s) {
            continue;
        }
        if name == "target" {
            if is_build_dir(&path) && path != root.join("target") {
                out.push(StrayTarget {
                    path:  path.display().to_string(),
                    bytes: size(&path),
                });
            }
            continue;
        }
        walk(root, &path, depth + 1, out);
    }
}

fn is_build_dir(dir: &Path) -> bool {
    dir.join("CACHEDIR.TAG").exists()
        || dir.join(".rustc_info.json").exists()
        || dir.join("debug").is_dir()
        || dir.join("release").is_dir()
}

/// Bytes under `dir`, links not followed.
fn size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            match e.file_type() {
                Ok(t) if t.is_dir() => size(&e.path()),
                Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => 0,
            }
        })
        .sum()
}

/// A size for a person: binary units, one decimal from a megabyte up.
pub fn human(bytes: u64) -> String {
    const K: u64 = 1024;
    match bytes {
        b if b >= K * K * K => format!("{:.1} GiB", b as f64 / (K * K * K) as f64),
        b if b >= K * K => format!("{:.1} MiB", b as f64 / (K * K) as f64),
        b if b >= K => format!("{} KiB", b / K),
        b => format!("{b} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plant(root: &Path, rel: &str) {
        fs::create_dir_all(root.join(rel)).unwrap();
    }

    fn paths(found: &[StrayTarget], root: &Path) -> Vec<String> {
        found
            .iter()
            .map(|s| {
                s.path
                    .strip_prefix(&format!("{}/", root.display()))
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn the_workspaces_own_target_alone_is_nothing() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "target/debug");
        plant(d.path(), "repo/src");
        assert!(collect(d.path()).is_empty());
    }

    #[test]
    fn a_worktrees_and_a_nested_build_directory_are_listed_in_path_order() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "target/debug");
        plant(d.path(), ".worktrees/w/target/debug");
        plant(d.path(), "repo/mock/target/mockspace-bench-test");
        fs::write(d.path().join("repo/mock/target/CACHEDIR.TAG"), "x").unwrap();
        let found = collect(d.path());
        assert_eq!(paths(&found, d.path()), [
            ".worktrees/w/target",
            "repo/mock/target"
        ]);
    }

    #[test]
    fn a_source_directory_called_target_is_not_a_build_directory() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "repo/src/target");
        fs::write(d.path().join("repo/src/target/lib.rs"), "x").unwrap();
        assert!(collect(d.path()).is_empty());
    }

    #[test]
    fn the_size_counts_every_file_below_it() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "w/target/debug/deps");
        fs::write(d.path().join("w/target/debug/a"), vec![0u8; 1000]).unwrap();
        fs::write(d.path().join("w/target/debug/deps/b"), vec![0u8; 24]).unwrap();
        assert_eq!(collect(d.path())[0].bytes, 1024);
    }

    #[test]
    fn git_and_node_modules_are_not_entered() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), ".git/target/debug");
        plant(d.path(), "web/node_modules/x/target/debug");
        assert!(collect(d.path()).is_empty());
    }

    #[test]
    fn a_build_directory_past_the_depth_is_not_found_and_one_at_it_is() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "a/b/c/d/e/target/debug");
        plant(d.path(), "a/b/c/d/e/f/target/debug");
        let found = collect(d.path());
        assert_eq!(paths(&found, d.path()), ["a/b/c/d/e/target"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_build_directory_is_not_followed() {
        let d = tempfile::tempdir().unwrap();
        plant(d.path(), "elsewhere/debug");
        plant(d.path(), "w");
        std::os::unix::fs::symlink(d.path().join("elsewhere"), d.path().join("w/target")).unwrap();
        assert!(collect(d.path()).is_empty());
    }

    #[test]
    fn sizes_are_written_in_binary_units() {
        assert_eq!(human(10), "10 B");
        assert_eq!(human(2048), "2 KiB");
        assert_eq!(human(3 * 1024 * 1024 + 512 * 1024), "3.5 MiB");
        assert_eq!(human(26 * 1024 * 1024 * 1024), "26.0 GiB");
    }
}
