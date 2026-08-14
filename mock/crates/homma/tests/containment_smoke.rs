//! Where a workspace may and may not be created, end to end.
//!
//! Split from `org_smoke.rs`, which crossed the file-size limit as these grew.
//!
//! Every test here is a reproduction that shipped. Four consecutive rounds each
//! closed one route into another repository's working tree and left the next
//! open: the current directory, a relative configuration path, a relative
//! workspace field, and a workspace escaping with `..`. They are gathered in
//! one file because they are one property, and keeping them apart is part of
//! why each was found separately.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
}

/// A content repository with one commit, and a clone of it to run homma from.
///
/// Built with the git binary because it is scaffolding, not the thing tested.
fn content_repo_and_root(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let run = |args: &[&str], at: &std::path::Path| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(at)
            .status()
            .expect("git should run")
            .success();
        assert!(ok, "git {args:?} failed");
    };
    let src = dir.join("content");
    std::fs::create_dir_all(&src).unwrap();
    run(&["init", "-q", "-b", "main"], &src);
    run(&["config", "user.name", "src"], &src);
    run(&["config", "user.email", "src@example.invalid"], &src);
    std::fs::write(src.join("README.md"), "content").unwrap();
    run(&["add", "README.md"], &src);
    run(&["commit", "-q", "-m", "initial", "--no-gpg-sign"], &src);

    let root = dir.join("root");
    run(
        &["clone", "-q", src.to_str().unwrap(), root.to_str().unwrap()],
        dir,
    );
    std::fs::write(
        root.join("homma.toml"),
        format!("content_repo = \"{}\"\n", src.display()),
    )
    .unwrap();
    (src, root)
}

#[test]
fn a_relative_workspace_anchors_at_the_root_and_not_at_the_process() {
    // The route three rounds never looked at: `workspace = "hands/rel"` was
    // used raw as a clone target, so it resolved against whatever directory the
    // process was in. Run from inside a committed repository it cloned a nested
    // repository into that repository's tree and exited 0.
    //
    // `../out/rel` rather than `hands/rel`, because a workspace is a *clone* of
    // the content repository and cannot live inside it; see the test below.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    // A different directory to stand in, and a repository, so a stray write shows.
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&elsewhere)
        .status()
        .unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "r"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "r", "--git-email", "r@example.invalid"])
        .args(["--workspace", "../out/rel"])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .current_dir(&elsewhere)
        .assert()
        .success();

    assert!(
        root.parent().unwrap().join("out/rel/.git").exists(),
        "a relative workspace anchors at the workspace root"
    );
    assert!(
        !elsewhere.join("out").exists() && !elsewhere.join("hands").exists(),
        "and never at whatever directory the process happened to be in"
    );
}

#[test]
fn a_workspace_inside_the_content_repository_is_refused() {
    // A workspace is a clone of the content repository, so it cannot live in
    // its working tree: that is a repository inside a repository, which is the
    // thing the deny list forbids and which `..` reached from the other side.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "r"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "r", "--git-email", "r@example.invalid"])
        .args(["--workspace", "hands/rel"])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sits inside the repository"));

    assert!(!root.join("hands").exists(), "and nothing is created");
}

#[test]
fn a_workspace_escaping_into_another_repository_is_refused() {
    // The reproduction the fifth review found: `AbsPath` made the path
    // absolute and said nothing about where it pointed, so `..` walked
    // straight back out into an unrelated committed repository and a clone
    // landed in its working tree. Exit 0.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    let victim = dir.path().join("victim");
    std::fs::create_dir_all(&victim).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&victim)
        .status()
        .unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "r"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "r", "--git-email", "r@example.invalid"])
        .args(["--workspace", "../victim/stolen"])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sits inside the repository"));

    assert!(
        !victim.join("stolen").exists(),
        "nothing may be written into another repository's tree"
    );
}

#[test]
fn standing_up_does_not_depend_on_where_the_operator_is_standing() {
    // Reproduced before this existed: `--root` defaulted to the current
    // directory rather than the configuration's own, so running this from an
    // unrelated clone cloned that repository into the Hand's workspace and
    // wrote the Hand's directories into the unrelated clone's tree.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    let ws = dir.path().join("ws").join("fresh");

    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "fresh"])
        .args(["--role", "hand", "--staffed"])
        .args([
            "--git-name",
            "fresh",
            "--git-email",
            "fresh@example.invalid",
        ])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "fresh"])
        .current_dir(&elsewhere)
        .assert()
        .success();

    // The workspace is a clone of the configured content repository.
    assert!(ws.join(".git").exists());
    assert!(
        std::fs::read_to_string(ws.join("README.md"))
            .unwrap()
            .contains("content"),
        "the configured repository must be what was cloned"
    );

    // And nothing was written where the operator happened to be standing.
    assert!(
        !elsewhere.join(".shared").exists() && !elsewhere.join(".claude").exists(),
        "the current directory is not the workspace and must not be written to"
    );
    // It went to the configuration's own directory instead.
    assert!(root.join(".shared/hands/fresh").exists());
}
