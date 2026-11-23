//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The warn rows no other test fires: the branch and remote rows over a repository with a bare remote
//! and a `dev` branch, the tag and ref rows, the history rows and the sequence rows. Each row has the case
//! that fires it and the case beside it that leaves it quiet, so a row that fires on everything or on
//! nothing fails one of the two.

use super::tests::{clean, commit, run, run_on, sh};
use super::*;
use crate::release::registry::Registry;

/// The repository, and the bare remote it pushes to; both live as long as this does.
struct Remote {
    work: tempfile::TempDir,
    bare: tempfile::TempDir,
}

impl Remote {
    fn p(&self) -> &Path {
        self.work.path()
    }
}

/// A clean repo on `main` with a bare `origin` holding `main` and `dev` at the same commit, both
/// tracked.
fn with_remote() -> Remote {
    let work = clean();
    let bare = tempfile::tempdir().unwrap();
    sh(bare.path(), &["init", "-q", "--bare", "-b", "main"]);
    let p = work.path();
    sh(p, &[
        "remote",
        "add",
        "origin",
        bare.path().to_str().unwrap(),
    ]);
    sh(p, &["branch", "dev"]);
    sh(p, &["push", "-q", "-u", "origin", "main", "dev"]);
    Remote {
        work,
        bare,
    }
}

fn clone_of(bare: &Path) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    sh(d.path(), &["clone", "-q", bare.to_str().unwrap(), "."]);
    for args in [["config", "user.email", "t@t"], ["config", "user.name", "t"], [
        "config",
        "commit.gpgsign",
        "false",
    ]] {
        sh(d.path(), &args);
    }
    d
}

/// The messages of the findings with this id.
fn of<'a>(found: &'a [String], id: &str) -> Vec<&'a String> {
    found
        .iter()
        .filter(|f| f.split(' ').next() == Some(id))
        .collect()
}

fn fires(found: &[String], id: &str) {
    assert!(!of(found, id).is_empty(), "{id} did not fire: {found:?}");
}

fn quiet(found: &[String], id: &str) {
    assert!(of(found, id).is_empty(), "{id} fired: {found:?}");
}

/// A commit whose committer date is `when`, so the order of tags by date can be set.
fn commit_dated(d: &Path, path: &str, body: &str, when: &str) {
    std::fs::write(d.join(path), body).unwrap();
    sh(d, &["add", path]);
    let out = std::process::Command::new("git")
        .current_dir(d)
        .env("GIT_COMMITTER_DATE", when)
        .env("GIT_AUTHOR_DATE", when)
        .args(["commit", "-q", "-m", &format!("feat: {when}")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn manifest(version: &str) -> String {
    format!("[package]\nname = \"x\"\nversion = \"{version}\"\n")
}

#[test]
fn a_clean_repo_with_its_remote_and_dev_raises_nothing() {
    let r = with_remote();
    assert_eq!(run(r.p(), &Published::default()), Vec::<String>::new());
}

#[test]
fn commits_on_the_remote_that_are_not_here_are_counted() {
    let r = with_remote();
    quiet(&run(r.p(), &Published::default()), "tree.uptodate");
    let other = clone_of(r.bare.path());
    commit(other.path(), "NEWS", "n\n", "docs: news");
    sh(other.path(), &["push", "-q", "origin", "main"]);
    sh(r.p(), &["fetch", "-q", "origin"]);
    let found = run(r.p(), &Published::default());
    let msg = of(&found, "tree.uptodate");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("1 commit(s) on `origin/main`"),
        "{}",
        msg[0]
    );
}

#[test]
fn a_worktree_still_checked_out_is_named_by_its_path() {
    let r = with_remote();
    quiet(&run(r.p(), &Published::default()), "tree.worktrees");
    let beside = tempfile::tempdir().unwrap();
    let at = beside.path().join("wt");
    sh(r.p(), &[
        "worktree",
        "add",
        "-q",
        at.to_str().unwrap(),
        "-b",
        "side",
    ]);
    let found = run(r.p(), &Published::default());
    let msg = of(&found, "tree.worktrees");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains(at.to_str().unwrap()), "{}", msg[0]);
}

#[test]
fn a_working_trunk_that_is_the_release_branch_is_named() {
    let r = with_remote();
    quiet(
        &run_on(r.p(), &Published::default(), "dev", "main"),
        "branch.trunk",
    );
    let found = run_on(r.p(), &Published::default(), "main", "main");
    let msg = of(&found, "branch.trunk");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains("`main` itself"), "{}", msg[0]);
}

#[test]
fn a_remote_branch_the_trunk_holds_is_merged_and_not_unmerged() {
    let r = with_remote();
    quiet(&run(r.p(), &Published::default()), "branch.merged");
    sh(r.p(), &["push", "-q", "origin", "dev:refs/heads/feat/done"]);
    let found = run(r.p(), &Published::default());
    let msg = of(&found, "branch.merged");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains("`origin/feat/done`"), "{}", msg[0]);
    quiet(&found, "branch.unmerged");
}

#[test]
fn a_remote_branch_with_commits_the_trunk_lacks_is_unmerged_and_not_merged() {
    let r = with_remote();
    quiet(&run(r.p(), &Published::default()), "branch.unmerged");
    sh(r.p(), &["checkout", "-q", "-b", "feat/new"]);
    commit(r.p(), "NEW", "n\n", "feat: new");
    sh(r.p(), &["push", "-q", "-u", "origin", "feat/new"]);
    let found = run(r.p(), &Published::default());
    let msg = of(&found, "branch.unmerged");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("`origin/feat/new` has 1 unmerged commit(s)"),
        "{}",
        msg[0]
    );
    quiet(&found, "branch.merged");
}

#[test]
fn a_local_branch_whose_upstream_is_gone_is_an_orphan() {
    let r = with_remote();
    sh(r.p(), &["branch", "side"]);
    sh(r.p(), &["push", "-q", "-u", "origin", "side"]);
    quiet(&run(r.p(), &Published::default()), "branch.orphan");
    sh(r.p(), &["push", "-q", "origin", "--delete", "side"]);
    sh(r.p(), &["fetch", "-q", "--prune", "origin"]);
    let found = run(r.p(), &Published::default());
    let msg = of(&found, "branch.orphan");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains("`side`"), "{}", msg[0]);
}

#[test]
fn rewrite_plumbing_left_behind_is_named() {
    let d = clean();
    quiet(&run(d.path(), &Published::default()), "ref.filterrepo");
    sh(d.path(), &[
        "update-ref",
        "refs/original/refs/heads/main",
        "HEAD",
    ]);
    let found = run(d.path(), &Published::default());
    let msg = of(&found, "ref.filterrepo");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("refs/original/refs/heads/main"),
        "{}",
        msg[0]
    );
}

#[test]
fn a_lower_version_committed_after_a_higher_one_is_out_of_order() {
    // the higher tag sits on a commit dated before the lower tag's
    let d = clean();
    let p = d.path();
    commit_dated(p, "Cargo.toml", &manifest("0.2.0"), "2000-01-01T00:00:00Z");
    sh(p, &["tag", "-a", "v0.2.0", "-m", "v0.2.0"]);
    let found = run(p, &Published::default());
    let msg = of(&found, "tag.order.matches");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("0.1.0 was committed after the higher 0.2.0"),
        "{}",
        msg[0]
    );
}

#[test]
fn tags_committed_in_version_order_are_in_order() {
    let d = clean();
    let p = d.path();
    commit_dated(p, "Cargo.toml", &manifest("0.2.0"), "2090-01-01T00:00:00Z");
    sh(p, &["tag", "-a", "v0.2.0", "-m", "v0.2.0"]);
    quiet(&run(p, &Published::default()), "tag.order.matches");
}

#[test]
fn a_commit_that_touched_a_workflow_is_counted() {
    let d = clean();
    quiet(&run(d.path(), &Published::default()), "hist.workflow.past");
    commit(
        d.path(),
        ".github/workflows/ci.yml",
        "on: push\n",
        "ci: add",
    );
    let found = run(d.path(), &Published::default());
    let msg = of(&found, "hist.workflow.past");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains("1 commit(s)"), "{}", msg[0]);
}

#[test]
fn a_file_over_the_limit_is_named_and_one_at_the_limit_is_not() {
    let d = clean();
    let at_limit = "a".repeat(LARGE_BLOB as usize);
    commit(d.path(), "blob/at.bin", &at_limit, "chore: at the limit");
    quiet(&run(d.path(), &Published::default()), "hist.large");
    let over = "a".repeat(LARGE_BLOB as usize + 1);
    commit(d.path(), "blob/over.bin", &over, "chore: over the limit");
    let found = run(d.path(), &Published::default());
    let msg = of(&found, "hist.large");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("blob/over.bin") && msg[0].contains("1000001 bytes"),
        "{}",
        msg[0]
    );
}

#[test]
fn a_tag_that_reads_like_a_version_and_is_not_one_is_named() {
    let d = clean();
    let p = d.path();
    // names that do not look like a version are the stray row's business and not this one's
    sh(p, &["tag", "-a", "archive/old", "-m", "x"]);
    sh(p, &["tag", "-a", "latest", "-m", "x"]);
    let found = run(p, &Published::default());
    quiet(&found, "semver.parses");
    fires(&found, "tag.stray");
    sh(p, &["tag", "-a", "v1.2", "-m", "x"]);
    let found = run(p, &Published::default());
    let msg = of(&found, "semver.parses");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(msg[0].contains("`v1.2`"), "{}", msg[0]);
}

fn published_x(versions: &[&str]) -> Published {
    let mut published = Published::default();
    published.versions.insert(
        (Registry::CratesIo, "x".into()),
        versions.iter().map(|v| v.parse().unwrap()).collect(),
    );
    published
}

#[test]
fn a_hole_in_a_registry_no_tags_could_close_is_named() {
    let d = clean();
    let found = run(d.path(), &published_x(&["0.1.0", "0.3.0"]));
    let msg = of(&found, "semver.gaps.closable");
    assert_eq!(msg.len(), 1, "{found:?}");
    assert!(
        msg[0].contains("x on crates-io") && msg[0].contains("0.1.0 to 0.3.0"),
        "{}",
        msg[0]
    );
}

#[test]
fn a_hole_the_tags_bridge_is_not_named_and_neither_is_a_riders_hole() {
    let d = clean();
    let p = d.path();
    sh(p, &["tag", "-a", "v0.2.0", "-m", "v0.2.0"]);
    quiet(
        &run(p, &published_x(&["0.1.0", "0.3.0"])),
        "semver.gaps.closable",
    );
    // a hole with no tags to close it, in a package whose versions are not the tags'
    let tags = git::tags(p).unwrap();
    let versioned: Vec<(String, Version)> = vec![("v0.1.0".into(), "0.1.0".parse().unwrap())];
    let tracked: Vec<String> = Vec::new();
    let published = published_x(&["0.1.0", "0.5.0"]);
    let found: Vec<String> = findings(&Seen {
        root:      p,
        remote:    "origin",
        trunk:     "dev",
        release:   "main",
        tags:      &tags,
        versioned: &versioned,
        tracked:   &tracked,
        published: &published,
        riders:    &["x".to_string()],
        manifest:  None,
    })
    .unwrap()
    .into_iter()
    .map(|f| format!("{} {}", f.id, f.message))
    .collect();
    quiet(&found, "semver.gaps.closable");
}
