//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma milestone` through the binary, over repositories built here.

mod support;

use std::path::Path;

use predicates::prelude::*;
use support::{bin, committed_crate, git_in, minimal_config_toml};

/// A workspace with the repo `x` on `main`, one commit in, and the manifest.
fn workspace() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    let cfg = cfg.to_str().unwrap().to_string();
    (dir, cfg)
}

fn commit_file(root: &Path, path: &str, content: &str) -> String {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, content).unwrap();
    git_in(root, &["add", path]);
    git_in(root, &["commit", "-qm", &format!("test: edit {path}")]);
    let out = support::git()
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn milestone_is_listed_and_a_mark_is_quiet_while_base_prints_the_sha() {
    let (dir, cfg) = workspace();
    let root = dir.path().join("x");
    bin()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("milestone"));
    let first = commit_file(&root, "a.txt", "one\n");
    bin()
        .args(["-c", &cfg, "milestone", "base", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no milestone is marked and no `milestone/*` tag is on the trunk; `homma milestone mark <repo> <sha>` records one",
        ));
    bin()
        .args(["-c", &cfg, "milestone", "mark", "x", &first])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    // base reads what the trunk commits, so the caller's commit comes first
    bin()
        .args(["-c", &cfg, "milestone", "base", "x"])
        .assert()
        .failure();
    git_in(&root, &["add", ".homma/milestones.toml"]);
    git_in(&root, &["commit", "-qm", "test: record the milestone"]);
    bin()
        .args(["-c", &cfg, "milestone", "base", "x"])
        .assert()
        .success()
        .stdout(format!("{first}\n"));
    bin()
        .args(["-c", &cfg, "--output", "json", "milestone", "base", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"source\": \"record\""))
        .stdout(predicate::str::contains(&first));
    bin()
        .args(["-c", &cfg, "milestone", "mark", "x", &first])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is already marked"));
    bin()
        .args(["-c", &cfg, "milestone", "base", "nowhere"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("`nowhere` is not a repository"));
}

#[test]
fn the_range_reports_print_partitions_and_stand_ins_and_refuse_a_wrong_base() {
    let (dir, cfg) = workspace();
    let root = dir.path().join("x");
    let start = commit_file(&root, "seed.txt", "x\n");
    commit_file(
        &root,
        "mock/crates/k/src/lib.rs",
        "fn a() {}\n// FIXME: later\n",
    );
    commit_file(&root, "docs/guide.md", "g\n");
    bin()
        .args(["-c", &cfg, "milestone", "partition", "x", &start, "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2  k source  (1 file, +2 -0)"))
        .stdout(predicate::str::contains(
            "1  (root) document  (1 file, +1 -0)",
        ));
    bin()
        .args(["-c", &cfg, "milestone", "stand-ins", "x", &start, "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "mock/crates/k/src/lib.rs:2  FIXME  // FIXME: later",
        ));
    bin()
        .args(["-c", &cfg, "--output", "json", "milestone", "stand-ins", "x", &start, "HEAD"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"marker\": \"FIXME\""));
    bin()
        .args(["-c", &cfg, "milestone", "partition", "x", "HEAD", &start])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not an ancestor of"));
}

#[test]
fn branches_prints_each_branch_the_trunk_lacks_with_its_conflicts_and_call() {
    let (dir, cfg) = workspace();
    let root = dir.path().join("x");
    commit_file(&root, "shared.txt", "line\n");
    git_in(&root, &["switch", "-qc", "clean"]);
    commit_file(&root, "c.txt", "a\n");
    git_in(&root, &["switch", "-q", "main"]);
    git_in(&root, &["switch", "-qc", "clash"]);
    commit_file(&root, "shared.txt", "branch\n");
    git_in(&root, &["switch", "-q", "main"]);
    commit_file(&root, "shared.txt", "trunk\n");
    git_in(&root, &["switch", "-q", "--orphan", "island"]);
    commit_file(&root, "island.txt", "alone\n");
    git_in(&root, &["switch", "-q", "main"]);
    bin()
        .args(["-c", &cfg, "milestone", "branches", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "clash  merge-trunk  2 lines  1 commits  conflicts: shared.txt",
        ))
        .stdout(predicate::str::contains(
            "island  unrelated  0 lines  1 commits",
        ))
        .stdout(predicate::str::contains("clean  land  1 lines  1 commits"));
    bin()
        .args(["-c", &cfg, "--output", "json", "milestone", "branches", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"call\": \"merge-trunk\""));
}

/// A directory holding a `git` that fails `merge-base` the way a broken
/// repository does, exit 128, and hands every other call to the real one.
#[cfg(unix)]
fn git_that_cannot_find_a_merge_base(dir: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let real = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap().trim().to_string();
    let shim = dir.join("git");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nif [ \"$1\" = merge-base ] && [ \"$2\" != --is-ancestor ]; then\n  echo 'fatal: shim refuses merge-base' >&2\n  exit 128\nfi\nexec {real} \"$@\"\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[cfg(unix)]
#[test]
fn a_branch_git_cannot_compare_is_listed_unreadable_with_its_reason_shown() {
    let (dir, cfg) = workspace();
    let root = dir.path().join("x");
    commit_file(&root, "shared.txt", "line\n");
    git_in(&root, &["switch", "-qc", "side"]);
    commit_file(&root, "s.txt", "a\n");
    git_in(&root, &["switch", "-q", "main"]);
    let shims = tempfile::tempdir().unwrap();
    let path = git_that_cannot_find_a_merge_base(shims.path());
    bin()
        .env("PATH", &path)
        .args(["-c", &cfg, "milestone", "branches", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "side  unreadable  0 lines  1 commits  reason: fatal: shim refuses merge-base",
        ));
    bin()
        .env("PATH", &path)
        .args(["-c", &cfg, "--output", "json", "milestone", "branches", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"call\": \"unreadable\""))
        .stdout(predicate::str::contains(
            "\"reason\": \"fatal: shim refuses merge-base\"",
        ));
}
