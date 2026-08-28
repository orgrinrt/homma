//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke tests for the gix-backed `RepoOps` impl.
//!
//! Tests construct repositories via `gix::init`, drive ops, and assert
//! outcomes. No network. Each test is hermetic to its own tempdir.

use std::path::Path;
use std::process::Command;

use homma_core::{GixRepo, RepoOps};
use tempfile::TempDir;

/// Initialise a fresh repo with one commit on `main`. Returns the
/// tempdir guard (must outlive the test) and the path to the repo root.
fn init_with_one_commit() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    gix::init(dir.path()).expect("git init");
    // The git CLI builds the fixture rather than gix. Keeps it simple, and it
    // means the repository these tests drive was made by the thing that has to
    // be able to read it afterwards.
    run_git(dir.path(), &["config", "user.name", "Test"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("README.md"), "hello\n").unwrap();
    run_git(dir.path(), &["add", "README.md"]);
    run_git(dir.path(), &["commit", "-m", "initial"]);
    // Make sure the branch is named `main` regardless of the user's git
    // default; older git versions still default to `master`.
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    if head.trim() != "main" {
        run_git(dir.path(), &["branch", "-M", "main"]);
    }
    dir
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git invocable");
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

/// Run git and return its trimmed stdout, failing loudly on a non-zero exit.
///
/// Separate from `run_git` because the point here is the answer, not that the
/// command worked, and a silent empty string would make an assertion pass for
/// the wrong reason.
fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("running git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("git stdout is utf8")
        .trim()
        .to_string()
}

#[test]
fn opens_existing_repo() {
    let dir = init_with_one_commit();
    let repo = GixRepo::open(dir.path()).expect("open");
    assert_eq!(repo.root(), dir.path());
}

#[test]
fn current_branch_reports_main() {
    let dir = init_with_one_commit();
    let repo = GixRepo::open(dir.path()).expect("open");
    let branch = repo.current_branch().expect("current_branch");
    assert_eq!(branch.as_deref(), Some("main"));
}

#[test]
fn branches_lists_local_heads() {
    let dir = init_with_one_commit();
    run_git(dir.path(), &["branch", "feat/x"]);
    let repo = GixRepo::open(dir.path()).expect("open");
    let mut names: Vec<String> = repo
        .branches()
        .expect("branches")
        .into_iter()
        .map(|b| b.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["feat/x".to_string(), "main".to_string()]);
}

#[test]
fn status_reports_clean_then_dirty() {
    let dir = init_with_one_commit();
    let repo = GixRepo::open(dir.path()).expect("open");
    let clean = repo.status().expect("status clean");
    assert!(clean.is_clean, "freshly committed tree should be clean");
    assert_eq!(clean.worktree_changes, 0);
    assert_eq!(clean.current_branch.as_deref(), Some("main"));

    // Dirty the tree.
    std::fs::write(dir.path().join("README.md"), "changed\n").unwrap();
    let dirty = repo.status().expect("status dirty");
    assert!(!dirty.is_clean, "modified file should be dirty");
}

#[test]
fn remotes_round_trip_add_then_remove() {
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    assert!(repo.remotes().expect("remotes empty").is_empty());

    repo.add_remote("origin", "https://example.invalid/repo.git")
        .expect("add origin");
    let after_add = repo.remotes().expect("remotes after add");
    assert_eq!(after_add.len(), 1);
    assert_eq!(after_add[0].name, "origin");
    assert_eq!(after_add[0].url, "https://example.invalid/repo.git");

    repo.remove_remote("origin").expect("remove origin");
    assert!(repo.remotes().expect("remotes after remove").is_empty());
}

#[test]
fn create_branch_from_main() {
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    repo.create_branch("feat/y", "main").expect("create_branch");
    let names: Vec<String> = repo
        .branches()
        .expect("branches")
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert!(names.contains(&"feat/y".to_string()));
}

#[test]
fn checkout_switches_head_pointer() {
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    repo.create_branch("feat/z", "main").expect("create_branch");
    repo.checkout("feat/z").expect("checkout");
    assert_eq!(
        repo.current_branch().expect("current_branch").as_deref(),
        Some("feat/z")
    );
}

#[test]
fn checkout_unknown_branch_fails() {
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    let err = repo.checkout("nope").unwrap_err();
    assert!(format!("{err}").contains("branch not found"));
}

#[test]
fn what_add_remote_writes_is_what_git_itself_reads_back() {
    // `remotes_round_trip_add_then_remove` writes through gix and reads back
    // through gix, so a wrong key spelling agrees with itself and passes. Ask
    // git instead, which is the implementation that actually has to read this
    // file, and check the fetch refspec too, which nothing else asserts at all.
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    repo.add_remote("upstream", "https://example.invalid/u.git")
        .expect("add upstream");

    assert_eq!(
        git_stdout(dir.path(), &["remote", "get-url", "upstream"]),
        "https://example.invalid/u.git",
        "git could not read back the url gix wrote"
    );
    assert_eq!(
        git_stdout(dir.path(), &["config", "--get", "remote.upstream.fetch"]),
        "+refs/heads/*:refs/remotes/upstream/*",
        "the fetch refspec is missing or malformed"
    );
    assert_eq!(
        git_stdout(dir.path(), &["remote"]),
        "upstream",
        "git sees a different set of remotes than the one that was written"
    );
}

#[test]
fn adding_a_remote_twice_leaves_one_section_and_the_second_url() {
    // The section is removed before it is written, so a second add replaces
    // rather than duplicating. A duplicate is invisible to `git remote get-url`,
    // which answers from the first match, so count the sections instead.
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    repo.add_remote("origin", "https://example.invalid/one.git")
        .expect("first add");
    repo.add_remote("origin", "https://example.invalid/two.git")
        .expect("second add");

    let config =
        std::fs::read_to_string(dir.path().join(".git").join("config")).expect("read .git/config");
    assert_eq!(
        config.matches("[remote \"origin\"]").count(),
        1,
        "the remote section was duplicated:\n{config}"
    );
    assert_eq!(
        git_stdout(dir.path(), &["remote", "get-url", "origin"]),
        "https://example.invalid/two.git",
        "the second add did not replace the first url"
    );
}

#[test]
fn remove_unknown_remote_fails() {
    let dir = init_with_one_commit();
    let mut repo = GixRepo::open(dir.path()).expect("open");
    let err = repo.remove_remote("origin").unwrap_err();
    assert!(format!("{err}").contains("remote not found"));
}
