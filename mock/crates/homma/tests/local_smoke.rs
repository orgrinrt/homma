//! `content_repo = "local"`, end to end.
//!
//! Split from `org_smoke.rs`, which crossed the file-size limit. These are the
//! cases where the workspace's own directory is the content repository, which
//! is the default and the shape a workspace starts in.

#[allow(dead_code, reason = "shared across test binaries; each uses a subset")]
mod support;

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
fn a_registry_with_no_content_repo_treats_its_own_directory_as_one() {
    // `local` is the default, so a workspace needs no configuration key at all
    // to start: the directory holding the registry becomes the content
    // repository, initialised if it is not one yet.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "# no content_repo key\n").unwrap();
    let ws = dir.path().join("hands").join("fresh");

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
        .assert()
        .success();

    assert!(root.join(".git").exists(), "the root must be initialised");
    assert!(ws.join(".git").exists(), "and the workspace cloned from it");
    let local = std::fs::read_to_string(ws.join(".git/config")).unwrap();
    assert!(local.contains("fresh@example.invalid"));
}

#[test]
fn local_may_be_said_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("b");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "b"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "b", "--git-email", "b@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "b"])
        .assert()
        .success();
    assert!(ws.join(".git").exists());
}

#[test]
fn a_local_root_that_is_already_a_repository_is_not_reinitialised() {
    // Standing up twice is the same answer, and initialising over an existing
    // repository would discard its history.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    // Point it at itself rather than at the remote it was cloned from.
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let before = std::fs::read_to_string(root.join(".git/config")).unwrap();
    let ws = dir.path().join("hands").join("c");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "c"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "c", "--git-email", "c@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "c"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(root.join(".git/config")).unwrap(),
        before,
        "an existing repository must not be re-initialised"
    );
}

#[test]
fn a_local_root_inside_another_repository_is_refused() {
    // Otherwise `local` initialises a repository inside somebody else's
    // checkout and writes a participant's directories into their tree, which
    // the deny list forbids outright.
    let dir = tempfile::tempdir().unwrap();
    let (_src, outer) = content_repo_and_root(dir.path());

    let nested = outer.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    let cfg = nested.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("h");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "h"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "h", "--git-email", "h@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "h"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sits inside the repository"));

    assert!(
        !nested.join(".git").exists(),
        "no repository may be initialised inside another one"
    );
    assert!(!ws.exists(), "and nothing may be cloned");
}

#[test]
fn a_relative_config_path_does_not_walk_out_of_the_nested_repository_guard() {
    // The bypass, exactly as reproduced. `enclosing_repo` walks `parent()`
    // upward; on a relative path the walk terminates at the empty string and
    // inspects the process's own directory instead, so the guard passed from
    // inside a committed repository and wrote a Hand's directories into it.
    //
    // The guard's other test passes an absolute path, which is the shape that
    // cannot fail. A guard tested only on that shape is not tested.
    let dir = tempfile::tempdir().unwrap();
    let (_src, outer) = content_repo_and_root(dir.path());

    let nested = outer.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("homma.toml"), "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("h");

    bin()
        .args(["--config", "homma.toml", "org", "add", "h"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "h", "--git-email", "h@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .current_dir(&nested)
        .assert()
        .success();

    // Relative config, invoked from the directory it names.
    bin()
        .args(["--config", "homma.toml", "org", "up", "h"])
        .current_dir(&nested)
        .assert()
        .failure();

    assert!(
        !nested.join(".git").exists(),
        "no repository may be initialised inside another one, however the \
         configuration path was spelled"
    );
    assert!(
        !nested.join(".shared").exists() && !nested.join(".claude").exists(),
        "and nothing may be written into the enclosing repository's tree"
    );
    assert!(!ws.exists(), "and nothing cloned");
}

#[test]
fn a_relative_config_path_still_stands_one_up_where_it_is_legitimate() {
    // The other half: refusing every relative path would be a fix that broke
    // the ordinary invocation instead of the defect.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("homma.toml"), "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("h");

    bin()
        .args(["--config", "homma.toml", "org", "add", "h"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "h", "--git-email", "h@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .current_dir(&root)
        .assert()
        .success();

    bin()
        .args(["--config", "homma.toml", "org", "up", "h"])
        .current_dir(&root)
        .assert()
        .success();

    assert!(root.join(".git").exists());
    assert!(ws.join(".git").exists());
}
