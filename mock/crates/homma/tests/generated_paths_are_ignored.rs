//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The repository's ignore configuration has the effect it exists for.
//!
//! `mock` rewrites the agent instructions under `.claude/` and `.github/` on any
//! normal invocation. They were untracked and unignored for long enough that a
//! workspace sweep found it rather than anybody here, which is the state where a
//! file is invisible to review, absent from history, and removed by a clean with
//! nothing left to recover from.
//!
//! The entries exist now. This is what keeps them working, and it asks
//! `git check-ignore` rather than reading `.gitignore`, so the check is over the
//! effect and not over the spelling.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Every path `mock` writes under the two agent directories.
const GENERATED: &[&str] = &[
    ".claude/rules",
    ".claude/skills",
    ".claude/hooks",
    ".claude/settings.json",
    ".github/instructions",
    ".github/skills",
    ".github/hooks",
    ".github/copilot-instructions.md",
];

/// A path a person writes by hand, beside the generated ones. It must stay
/// visible, and it is the only arm that tells a per-path ignore list apart from
/// a blanket on the two directories.
const HAND_WRITTEN: &str = ".github/workflows/ci.yml";

fn repo_root() -> PathBuf {
    // this crate sits at <root>/mock/crates/homma
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate is three levels below the repository root")
        .to_path_buf()
}

/// `Some(true)` ignored, `Some(false)` not, `None` when git could not answer,
/// which is not a statement about the property either way.
fn is_ignored(root: &Path, path: &str) -> Option<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "-q", "--no-index", path])
        .output()
        .ok()?;
    match out.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        // 128 is "not a git repository" and anything else is git failing to
        // answer. Either way the run says nothing about the ignore list.
        _ => None,
    }
}

fn in_a_work_tree(root: &Path) -> bool {
    is_ignored(root, ".gitignore").is_some()
}

#[test]
fn every_generated_path_is_ignored() {
    let root = repo_root();
    if !in_a_work_tree(&root) {
        eprintln!("skipped: {} is not a git work tree", root.display());
        return;
    }
    let visible: Vec<&str> = GENERATED
        .iter()
        .copied()
        .filter(|p| is_ignored(&root, p) == Some(false))
        .collect();
    assert!(
        visible.is_empty(),
        "mock rewrites these and nothing ignores them, so they sit untracked \
         and unignored: {visible:?}"
    );
}

#[test]
fn a_hand_written_workflow_is_not_ignored() {
    // the control on the test above. A blanket `.claude/` plus `.github/` passes
    // it completely while swallowing anything a person puts beside the generated
    // directories, so without this arm the weaker shape certifies as readily as
    // the stronger one.
    let root = repo_root();
    if !in_a_work_tree(&root) {
        eprintln!("skipped: {} is not a git work tree", root.display());
        return;
    }
    assert_eq!(
        is_ignored(&root, HAND_WRITTEN),
        Some(false),
        "{HAND_WRITTEN} is ignored, so the ignore list is a blanket on the \
         generated directories and a hand-written workflow put there would \
         never be seen"
    );
}

#[test]
fn the_check_can_report_both_answers() {
    // and the control on the instrument. Both assertions above rest on
    // `is_ignored` being able to return either answer; one that always said the
    // same thing would pass one of them and fail the other loudly, but one that
    // silently returned `None` would pass both by skipping.
    let root = repo_root();
    if !in_a_work_tree(&root) {
        eprintln!("skipped: {} is not a git work tree", root.display());
        return;
    }
    assert_eq!(
        is_ignored(&root, "target"),
        Some(true),
        "the build directory is ignored in every repository here"
    );
    assert_eq!(
        is_ignored(&root, "README.md"),
        Some(false),
        "a tracked file at the root is not ignored"
    );
}

#[test]
fn the_ignore_list_holds_before_anything_has_generated_into_it() {
    // A fresh clone is the ordinary state, and it is the one state the test
    // above cannot run under: it asks the repository root, where the generator
    // has already created every directory it names. Six of the patterns
    // carried a trailing slash, which matches only a path that exists as a
    // directory, so every one of them reported unignored on a clean checkout
    // and the whole workspace run aborted on the assertion.
    //
    // So this constructs the clean state rather than observing whichever state
    // the tree happens to be in: the same ignore file, and none of the output.
    let root = repo_root();
    if !in_a_work_tree(&root) {
        eprintln!("skipped: {} is not a git work tree", root.display());
        return;
    }
    let ignore = std::fs::read_to_string(root.join(".gitignore"))
        .expect("this repository has a .gitignore");

    let clean = tempfile::tempdir().unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "."])
        .current_dir(clean.path())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: could not init a temporary git repository");
        return;
    }
    std::fs::write(clean.path().join(".gitignore"), &ignore).unwrap();

    let unignored: Vec<&str> = GENERATED
        .iter()
        .copied()
        .filter(|p| is_ignored(clean.path(), p) == Some(false))
        .collect();
    assert!(
        unignored.is_empty(),
        "these are ignored only once something has created them, so a fresh \
         clone reports them untracked: {unignored:?}"
    );

    // The control. Without it this passes for a `check-ignore` answering zero
    // to everything, which is what a malformed ignore file or a git that could
    // not read one would produce.
    assert_eq!(
        is_ignored(clean.path(), "src/main.rs"),
        Some(false),
        "the temporary repository ignores a path nothing names, so the \
         assertion above says nothing"
    );
}
