//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A repository a test makes begins with no hooks, whatever the machine's git
//! config names as its template directory.
//!
//! `git init` copies `init.templateDir` into every new repository, and the
//! byline guard the container installs puts a `commit-msg` hook there. A
//! template whose hooks refuse everything is the worst case of that, so this
//! makes one, points a global configuration at it, and makes and commits a
//! repository the way every test in the workspace does: a `git` spawned with the
//! process environment as it is. The environment the test run pins decides
//! whether the hooks arrive.

use std::path::Path;
use std::process::Command;

/// A template directory whose `commit-msg` and `pre-commit` hooks exit 1, and a
/// global configuration naming it as `init.templateDir`.
fn hostile_global_config(dir: &Path) -> std::path::PathBuf {
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

#[test]
fn a_repository_made_under_a_hostile_template_dir_has_no_hooks_and_commits() {
    let d = tempfile::tempdir().unwrap();
    let config = hostile_global_config(d.path());
    let repo = d.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let init = git(&repo, &config, &["init", "-q", "-b", "main"]);
    assert!(init.status.success(), "git init: {init:?}");
    assert!(
        !repo.join(".git/hooks/commit-msg").exists(),
        "the repository carries the machine's commit-msg hook"
    );
    assert!(
        !repo.join(".git/hooks/pre-commit").exists(),
        "the repository carries the machine's pre-commit hook"
    );
    std::fs::write(repo.join("f"), "x").unwrap();
    let add = git(&repo, &config, &["add", "f"]);
    assert!(add.status.success(), "git add: {add:?}");
    let commit = git(&repo, &config, &["commit", "-qm", "feat: first"]);
    assert!(
        commit.status.success(),
        "a hostile hook refused the commit: {commit:?}"
    );
}
