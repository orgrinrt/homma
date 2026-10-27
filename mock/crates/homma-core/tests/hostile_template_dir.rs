//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A repository a test makes begins with no hooks, whatever the machine's git
//! config names as its template directory.
//!
//! `git init` and `git clone` copy `init.templateDir` into every new repository,
//! and the byline guard the container installs puts a `commit-msg` hook there. A
//! template whose hooks refuse everything is the worst case of that, so this
//! makes one, points a global configuration at it, and makes and commits
//! repositories the way every test in the workspace does: a `git` spawned with the
//! process environment as it is. The environment the test run pins decides
//! whether the hooks arrive.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A template directory whose `commit-msg` and `pre-commit` hooks exit 1, and a
/// global configuration naming it as `init.templateDir`.
fn hostile_global_config(dir: &Path) -> PathBuf {
    let hooks = dir.join("template").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for name in ["commit-msg", "pre-commit"] {
        let path = hooks.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let config = dir.join("hostile.gitconfig");
    std::fs::write(
        &config,
        format!(
            "[init]\n\ttemplateDir = {}\n[user]\n\tname = Test\n\temail = test@example.invalid\n",
            dir.join("template").display()
        ),
    )
    .unwrap();
    config
}

/// A `git` under the hostile configuration, with the process environment as
/// every test has it, bar the identity variables.
fn git(dir: &Path, config: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    homma_core::testing::without_git_identity_env(&mut cmd);
    cmd.args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", config)
        .env("GIT_CONFIG_SYSTEM", config)
        .output()
        .expect("git should run")
}

fn ok(out: &std::process::Output, what: &str) {
    assert!(out.status.success(), "{what}: {out:?}");
}

/// No template hook arrived in `repo`, and a commit in it is not refused.
fn assert_hookless_and_committable(repo: &Path, config: &Path, how: &str) {
    for name in ["commit-msg", "pre-commit"] {
        assert!(
            !repo.join(".git/hooks").join(name).exists(),
            "the repository made by {how} carries the machine's {name} hook"
        );
    }
    std::fs::write(repo.join("g"), "y").unwrap();
    ok(&git(repo, config, &["add", "g"]), "git add");
    let commit = git(repo, config, &["commit", "-qm", "feat: second"]);
    assert!(
        commit.status.success(),
        "a hostile hook refused a commit in the repository made by {how}: {commit:?}"
    );
}

#[test]
fn a_repository_made_under_a_hostile_template_dir_has_no_hooks_and_commits() {
    let d = tempfile::tempdir().unwrap();
    let config = hostile_global_config(d.path());
    let repo = d.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    ok(
        &git(&repo, &config, &["init", "-q", "-b", "main"]),
        "git init",
    );
    std::fs::write(repo.join("f"), "x").unwrap();
    ok(&git(&repo, &config, &["add", "f"]), "git add");
    let commit = git(&repo, &config, &["commit", "-qm", "feat: first"]);
    assert!(
        commit.status.success(),
        "a hostile hook refused the commit: {commit:?}"
    );
    assert_hookless_and_committable(&repo, &config, "git init");
}

#[test]
fn a_clone_made_under_a_hostile_template_dir_has_no_hooks_and_commits() {
    let d = tempfile::tempdir().unwrap();
    let config = hostile_global_config(d.path());
    let src = d.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    ok(
        &git(&src, &config, &["init", "-q", "-b", "main"]),
        "git init",
    );
    std::fs::write(src.join("f"), "x").unwrap();
    ok(&git(&src, &config, &["add", "f"]), "git add");
    ok(
        &git(&src, &config, &["commit", "-qm", "feat: first"]),
        "the first commit of the source",
    );
    let dst = d.path().join("dst");
    let clone = git(d.path(), &config, &[
        "clone",
        "-q",
        src.to_str().unwrap(),
        dst.to_str().unwrap(),
    ]);
    ok(&clone, "git clone");
    assert_hookless_and_committable(&dst, &config, "git clone");
}
