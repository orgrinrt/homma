//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Whether a repo's git hooks are wired at all.
//!
//! mockspace installs its hooks outside the repo and points `core.hooksPath` at
//! them, so that setting is the difference between every per-repo hook running
//! and none of them running. A repo with the setting unset has hooks on disk
//! that git never invokes, and nothing about the tree looks any different.
//!
//! It is read here rather than by running `git`, so a status pass over a
//! workspace is one process rather than one per member.

use std::path::Path;

/// The `core.hooksPath` in effect for the repo at `dir`, or `None`.
///
/// `None` covers three states that a caller does not need to tell apart,
/// because the consequence is the same in all of them: the path is not set, the
/// setting cannot be read, or `dir` is not a repository at all. Each means no
/// per-repo hook fires.
///
/// The value is read from the full configuration rather than the repository's
/// own file, so a machine that sets the path globally reads as wired, which it
/// is.
pub fn hooks_path_at(dir: &Path) -> Option<String> {
    let repo = gix::open(dir).ok()?;
    let value = repo.config_snapshot().string("core.hooksPath")?;
    let text = value.to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Whether any per-repo git hook fires in the repo at `dir`.
pub fn hooks_are_wired(dir: &Path) -> bool {
    hooks_path_at(dir).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real repository, made with gix rather than by writing a `.git` by
    /// hand, so what is read back is what git would read.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        gix::init(dir.path()).unwrap();
        dir
    }

    /// Set a key in the repository's own config file.
    ///
    /// Appended as text because that is what git itself writes, and it keeps
    /// the test independent of whichever gix version owns the write path.
    fn set(dir: &Path, key: &str, value: &str) {
        let at = dir.join(".git").join("config");
        let mut body = std::fs::read_to_string(&at).unwrap_or_default();
        body.push_str(&format!("[core]\n\t{key} = {value}\n"));
        std::fs::write(at, body).unwrap();
    }

    #[test]
    fn a_repo_with_no_hooks_path_reads_as_unwired() {
        let d = repo();
        assert_eq!(hooks_path_at(d.path()), None);
        assert!(!hooks_are_wired(d.path()));
    }

    #[test]
    fn a_repo_with_a_hooks_path_reads_as_wired_and_carries_the_value() {
        // The control on the case above: without this, the `None` there could
        // be a reader that never returns anything.
        let d = repo();
        set(d.path(), "hooksPath", "mock/target/hooks");
        assert_eq!(
            hooks_path_at(d.path()),
            Some("mock/target/hooks".to_string())
        );
        assert!(hooks_are_wired(d.path()));
    }

    #[test]
    fn an_absolute_hooks_path_is_read_whole() {
        // What mockspace actually sets: the durable hooks live outside the
        // repo, so the value is absolute and must not be truncated or
        // reinterpreted as relative.
        let d = repo();
        set(
            d.path(),
            "hooksPath",
            "/Users/somebody/.config/mockspace/hooks",
        );
        assert_eq!(
            hooks_path_at(d.path()),
            Some("/Users/somebody/.config/mockspace/hooks".to_string())
        );
    }

    #[test]
    fn a_directory_that_is_not_a_repository_reads_as_unwired_rather_than_failing() {
        // A status pass walks whatever the workspace holds, including a
        // directory somebody has not cloned into yet.
        let d = tempfile::tempdir().unwrap();
        assert_eq!(hooks_path_at(d.path()), None);
    }

    #[test]
    fn a_path_that_does_not_exist_reads_as_unwired() {
        assert_eq!(
            hooks_path_at(Path::new("/nonexistent/nowhere/at/all")),
            None
        );
    }

    #[test]
    fn an_empty_value_is_not_wired() {
        // `core.hooksPath =` with nothing after it is how the setting gets
        // turned off, and reading it as a path would report a repo as wired
        // while no hook fires.
        let d = repo();
        set(d.path(), "hooksPath", "");
        assert_eq!(hooks_path_at(d.path()), None);
        assert!(!hooks_are_wired(d.path()));
    }
}
