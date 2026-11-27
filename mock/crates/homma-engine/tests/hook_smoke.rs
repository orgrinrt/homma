//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! `homma hook` end to end: the git hooks are registered with git-shook and
//! declared nowhere in the manifest, a `[hooks]` left there is named and not
//! read, and what an older homma's entrypoints call gates the push and nothing
//! else.

use std::path::{Path, PathBuf};

use predicates::prelude::*;

mod support;
use support::{bin, git};

const MANIFEST: &str = "[workspace]\nname = \"w\"\npath = \".\"\n";

/// A workspace with one member repository, `r`, and the manifest written.
fn workspace(manifest: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    let member = root.join("r");
    std::fs::create_dir_all(&member).unwrap();
    for args in [&["init", "-q", "-b", "main"][..], &["config", "user.name", "t"], &[
        "config",
        "user.email",
        "t@t",
    ]] {
        assert!(
            git()
                .args(args)
                .current_dir(&member)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(root.join("homma.toml"), manifest).unwrap();
    (d, root, member)
}

fn homma(root: &Path) -> assert_cmd::Command {
    let mut c = bin();
    c.arg("--dir").arg(root);
    c
}

/// A directory holding one executable, `name`, that appends its arguments to
/// `log` and exits with `code`.
fn recorder(name: &str, code: i32) -> (tempfile::TempDir, PathBuf) {
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("called");
    let path = bin.path().join(name);
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\ncat >> '{}.stdin'\nexit {code}\n",
            log.display(),
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (bin, log)
}

fn path_with(dir: &Path) -> String {
    format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn install_where_git_shook_does_not_hold_the_hooks_path_refuses_and_says_what_to_do() {
    let (_d, root, member) = workspace(MANIFEST);
    homma(&root)
        .args(["hook", "install", "r"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("run `git shook install` there"));
    assert!(!member.join(".git/shook/manifests").exists());
}

#[test]
fn a_git_hooks_table_left_in_the_manifest_is_named_by_doctor_and_status_and_not_refused() {
    let (_d, root, _member) = workspace(&format!(
        "{MANIFEST}\n[[hooks.pre-commit]]\nrun = \"lintwhistle check {{paths}}\"\npaths = [\"*.md\"]\n"
    ));
    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "[hooks] is no longer read: the git hooks are declared in shook.toml. Move each row of it in homma.toml",
        ));
    homma(&root)
        .args(["status"])
        .assert()
        .stdout(predicate::str::contains(
            "  [hooks] is no longer read: the git hooks are declared in shook.toml",
        ));
    let doc = homma(&root)
        .args(["--output", "json", "status"])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    assert_eq!(
        doc["workspace"]["retired"],
        serde_json::json!([{ "table": "hooks", "file": "homma.toml" }])
    );

    // the same workspace without the table names nothing, which is the control
    std::fs::write(root.join("homma.toml"), MANIFEST).unwrap();
    homma(&root).args(["hook", "doctor"]).assert().success();
    let doc = homma(&root)
        .args(["--output", "json", "status"])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&doc.stdout).unwrap();
    assert_eq!(doc["workspace"]["retired"], serde_json::json!([]));
}

#[test]
fn doctor_says_where_the_git_hooks_are_declared() {
    let (_d, root, _member) = workspace(MANIFEST);
    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "no shook.toml at the workspace root",
        ));
    std::fs::write(root.join("shook.toml"), "name = \"prose\"\n").unwrap();
    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered as prose"));
    // and where `git_hooks` names another file, that file
    std::fs::write(
        root.join("homma.toml"),
        format!("{MANIFEST}\n[paths]\ngit_hooks = \"hooks/git.toml\"\n"),
    )
    .unwrap();
    homma(&root)
        .args(["hook", "doctor"])
        .assert()
        .stdout(predicate::str::contains(
            "no hooks/git.toml at the workspace root",
        ));
}

#[test]
fn doctor_with_a_repository_names_the_hooks_path_and_what_an_older_homma_left() {
    let (_d, root, member) = workspace(MANIFEST);
    let own = member.join(".git/hooks");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("pre-push"),
        "#!/bin/sh\n# homma writes this file; what it runs is [hooks] in homma.toml\nexec homma hook run pre-push \"$@\"\n",
    )
    .unwrap();
    homma(&root)
        .args(["hook", "doctor", "r"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "git-shook does not hold the hooks path",
        ))
        .stdout(predicate::str::contains("was left by an older homma"));
}

#[test]
fn an_older_entrypoint_where_git_shook_does_not_hold_the_path_fails_closed_and_runs_nothing() {
    for (event, stdin) in
        [("pre-commit", ""), ("pre-push", "refs/heads/main abc refs/heads/main def\n")]
    {
        let (_d, root, member) = workspace(MANIFEST);
        let (bin, log) = recorder("homma", 0);
        homma(&root)
            .current_dir(&member)
            .env("PATH", path_with(bin.path()))
            .args(["hook", "run", event])
            .write_stdin(stdin)
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "the {event} hook did not run"
            )))
            .stderr(predicate::str::contains("git-shook does not hold"));
        assert!(!log.exists(), "{event}: something ran");
    }
}

/// A clone git-shook holds, with a manifest of somebody else's and no install
/// of homma's yet, and `GIT_SHOOK`'s directory and `bin` on the `PATH`.
fn held_clone(shook_dir: &Path, bin: &Path) -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let (d, root, member) = workspace(MANIFEST);
    std::fs::write(
        root.join("shook.toml"),
        "name = \"prose\"\n\n[hooks.pre-commit]\nrun = [\"sh\", \"-c\", \"echo PROSE-CHECKED \\\"$@\\\"\", \"x\", \"{paths}\"]\npaths = [\"*.md\"]\n",
    )
    .unwrap();
    let path = format!("{}:{}", bin.display(), path_with(shook_dir));
    std::fs::create_dir_all(member.join(".git/shook/manifests")).unwrap();
    std::fs::write(
        member.join(".git/shook/manifests/other.toml"),
        "name = \"other\"\n\n[hooks.post-commit]\nrun = [\"true\"]\n",
    )
    .unwrap();
    let adopted = git()
        .args(["shook", "install", "--yes"])
        .current_dir(&member)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(adopted.status.success(), "{adopted:?}");
    (d, root, member, path)
}

/// An older entrypoint's call installs and hands the event to git-shook: the
/// workspace's check runs at `pre-commit` on what is staged, and the gate at
/// `pre-push` with git's arguments and stdin. Needs `GIT_SHOOK`.
#[test]
fn an_older_entrypoint_installs_and_hands_the_event_to_git_shook() {
    let Some(script) = std::env::var_os("GIT_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: GIT_SHOOK names no git-shook script");
        return;
    };
    let (bin, log) = recorder("homma", 0);
    let (_d, root, member, path) = held_clone(script.parent().unwrap(), bin.path());
    std::fs::write(member.join("notes.md"), "x").unwrap();
    assert!(
        git()
            .args(["add", "notes.md"])
            .current_dir(&member)
            .status()
            .unwrap()
            .success()
    );
    let out = homma(&root)
        .current_dir(&member)
        .env("PATH", &path)
        .args(["hook", "run", "pre-commit"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("PROSE-CHECKED notes.md"),
        "{out:?}"
    );
    assert!(
        member.join(".git/shook/manifests/homma.toml").is_file(),
        "the install ran"
    );
    homma(&root)
        .current_dir(&member)
        .env("PATH", &path)
        .args(["hook", "run", "pre-push", "origin", "https://example.invalid/r"])
        .write_stdin("refs/heads/main abc refs/heads/main def\n")
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        "release gate --hook origin https://example.invalid/r"
    );
    assert_eq!(
        std::fs::read_to_string(format!("{}.stdin", log.display())).unwrap(),
        "refs/heads/main abc refs/heads/main def\n",
        "the gate reads the refs git handed the hook"
    );
}

/// The control: a gate that refuses refuses the push through the same hand-on.
/// Needs `GIT_SHOOK`.
#[test]
fn a_gate_that_refuses_refuses_the_push_after_the_hand_on() {
    let Some(script) = std::env::var_os("GIT_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: GIT_SHOOK names no git-shook script");
        return;
    };
    let (bin, _log) = recorder("homma", 1);
    let (_d, root, member, path) = held_clone(script.parent().unwrap(), bin.path());
    homma(&root)
        .current_dir(&member)
        .env("PATH", &path)
        .args(["hook", "run", "pre-push", "origin", "https://example.invalid/r"])
        .write_stdin("")
        .assert()
        .failure();
}

/// The whole chain once, through the binary: git-shook holds the key, `hook
/// install` places the gate and the workspace's manifest, and a real commit
/// runs the workspace's entry for the paths it selects. Needs `GIT_SHOOK`
/// naming the git-shook script and says so when it is unset.
#[test]
fn the_workspace_hooks_installed_through_git_shook_run_on_a_real_commit() {
    let Some(script) = std::env::var_os("GIT_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: GIT_SHOOK names no git-shook script");
        return;
    };
    let shook_dir = script.parent().unwrap().to_path_buf();
    let (_d, root, member) = workspace(MANIFEST);
    std::fs::write(
        root.join("shook.toml"),
        "name = \"prose\"\n\n[hooks.pre-commit]\nrun = [\"sh\", \"-c\", \"echo PROSE-CHECKED \\\"$@\\\"\", \"x\", \"{paths}\"]\npaths = [\"*.md\"]\n",
    )
    .unwrap();
    let path = path_with(&shook_dir);
    // git-shook adopts a clone only when it has a manifest, so the key is taken
    // with a clone-local one that registers a harmless entry of someone else's
    std::fs::create_dir_all(member.join(".git/shook/manifests")).unwrap();
    std::fs::write(
        member.join(".git/shook/manifests/other.toml"),
        "name = \"other\"\n\n[hooks.post-commit]\nrun = [\"true\"]\n",
    )
    .unwrap();
    let adopted = git()
        .args(["shook", "install", "--yes"])
        .current_dir(&member)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(adopted.status.success(), "{adopted:?}");

    homma(&root)
        .env("PATH", &path)
        .args(["hook", "install", "r"])
        .assert()
        .success()
        .stdout(predicate::str::contains("homma, prose"));
    homma(&root)
        .env("PATH", &path)
        .args(["hook", "doctor", "r"])
        .assert()
        .success();

    std::fs::write(member.join("notes.md"), "x").unwrap();
    std::fs::write(member.join("code.rs"), "x").unwrap();
    assert!(
        git()
            .args(["add", "."])
            .current_dir(&member)
            .env("PATH", &path)
            .status()
            .unwrap()
            .success()
    );
    let committed = git()
        .args(["commit", "-m", "one"])
        .current_dir(&member)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(committed.status.success(), "{committed:?}");
    let said = String::from_utf8_lossy(&committed.stdout).into_owned()
        + &String::from_utf8_lossy(&committed.stderr);
    let checked: Vec<&str> = said
        .lines()
        .filter(|l| l.starts_with("PROSE-CHECKED"))
        .collect();
    assert_eq!(checked, vec!["PROSE-CHECKED notes.md"], "{said}");
}
