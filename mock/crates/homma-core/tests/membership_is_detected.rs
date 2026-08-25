//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which directories under a workspace root are members, and what is known
//! about each.
//!
//! Membership used to be a table somebody maintained, and the table spent a
//! month naming a crate that had been renamed while every reference in the
//! workspace followed it rather than the tree. So the tree is the answer now,
//! and these are the cases that decide what the tree is saying.

use std::path::Path;
use std::process::Command;

use homma_core::config::Config;

/// A config with the two forge profiles the cases below match against, and no
/// idea of membership until something detects it.
fn config() -> Config {
    Config::parse(
        r#"
[workspace]
name = "demo"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
"#,
    )
    .expect("the fixture manifest parses")
}

/// A real repository at `root/name`, with `origin` set to `url` when one is
/// given.
///
/// A real `git init` rather than a hand-made `.git` directory, because the
/// origin is read through git and a fake would answer nothing whatever the
/// detection did.
fn clone_at(root: &Path, name: &str, url: Option<&str>) {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    run(&["init", "-q"]);
    if let Some(url) = url {
        run(&["remote", "add", "origin", url]);
    }
}

#[test]
fn a_clone_is_a_member_and_its_remote_says_where_it_lives() {
    let root = tempfile::tempdir().unwrap();
    clone_at(
        root.path(),
        "notko",
        Some("https://github.com/orgrinrt/notko.git"),
    );
    clone_at(
        root.path(),
        "viola",
        Some("git@codeberg.org:hiisi/viola.git"),
    );

    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);

    let notko = cfg.repo("notko").expect("a clone is a member");
    assert_eq!(notko.forge.as_deref(), Some("github"));
    assert_eq!(notko.owner.as_deref(), Some("orgrinrt"));
    assert_eq!(notko.local_path.as_os_str(), "notko");

    let viola = cfg.repo("viola").expect("the ssh spelling reads too");
    assert_eq!(viola.forge.as_deref(), Some("codeberg"));
    assert_eq!(viola.owner.as_deref(), Some("hiisi"));
}

#[test]
fn a_clone_with_no_forge_is_still_a_member() {
    // Three ways a repository ends up without one, and all three are ordinary.
    // A declaration could not have expressed any of them: there was nothing to
    // write in the `forge` key but a guess.
    let root = tempfile::tempdir().unwrap();
    clone_at(root.path(), "fresh", None);
    clone_at(root.path(), "local", Some("/srv/git/local.git"));
    clone_at(
        root.path(),
        "elsewhere",
        Some("https://example.invalid/who/what.git"),
    );

    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);

    for name in ["fresh", "local", "elsewhere"] {
        let member = cfg
            .repo(name)
            .unwrap_or_else(|| panic!("{name} stopped being a member"));
        assert_eq!(member.forge, None, "{name} was given a forge");
        assert_eq!(member.local_path.as_os_str(), name);
    }
    // The owner is the one thing the last of the three does have: its remote
    // names one, on a host nothing here serves.
    assert_eq!(cfg.repo("elsewhere").unwrap().owner.as_deref(), Some("who"));
    assert_eq!(cfg.repo("fresh").unwrap().owner, None);
    assert_eq!(cfg.repo("local").unwrap().owner, None);
}

#[test]
fn a_directory_that_is_not_a_repository_is_not_a_member() {
    let root = tempfile::tempdir().unwrap();
    clone_at(
        root.path(),
        "real",
        Some("https://github.com/orgrinrt/real.git"),
    );
    std::fs::create_dir_all(root.path().join("notes")).unwrap();
    std::fs::create_dir_all(root.path().join("scripts/deep")).unwrap();
    std::fs::write(root.path().join("README.md"), "hello\n").unwrap();

    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);

    let names: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["real"], "something other than a clone got in");
}

#[test]
fn a_worktree_is_not_a_second_member() {
    // `git worktree add` writes a `.git` **file** pointing back at the clone's
    // object store. The tree looks exactly like a member from a distance, and
    // counting it would put one repository in the list twice under two names,
    // on two different branches, which every command that walks the list would
    // then act on separately.
    let root = tempfile::tempdir().unwrap();
    clone_at(
        root.path(),
        "notko",
        Some("https://github.com/orgrinrt/notko.git"),
    );
    let clone = root.path().join("notko");
    let run = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&clone)
            .args(args)
            .output()
            .expect("git runs")
    };
    // A worktree needs a commit to branch from.
    std::fs::write(clone.join("a"), "a\n").unwrap();
    run(&["add", "a"]);
    run(&[
        "-c",
        "user.email=t@example.invalid",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "one",
    ]);
    let out = run(&["worktree", "add", "-q", "../notko-seat", "-b", "seat"]);
    assert!(
        out.status.success(),
        "the fixture worktree was not created: {out:?}"
    );
    assert!(
        root.path().join("notko-seat/.git").is_file(),
        "the fixture is wrong: a worktree's .git should be a file"
    );

    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);

    let names: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["notko"], "the worktree was counted as a member");
}

#[test]
fn a_repository_inside_a_member_is_that_member_s_business() {
    let root = tempfile::tempdir().unwrap();
    clone_at(
        root.path(),
        "outer",
        Some("https://github.com/orgrinrt/outer.git"),
    );
    clone_at(
        &root.path().join("outer"),
        "vendored",
        Some("https://github.com/x/vendored.git"),
    );

    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);

    let names: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["outer"], "the walk went a level too deep");
}

#[test]
fn the_order_is_the_names_in_order() {
    let root = tempfile::tempdir().unwrap();
    for name in ["zeta", "alpha", "mu"] {
        clone_at(root.path(), name, None);
    }
    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);
    let names: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["alpha", "mu", "zeta"]);
}

#[test]
fn detecting_again_replaces_rather_than_accumulates() {
    // Two roots, one config. A union would report a workspace holding members
    // from a directory it is not in, which is worse than either answer alone.
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    clone_at(first.path(), "one", None);
    clone_at(second.path(), "two", None);

    let mut cfg = config();
    cfg.detect_members(first.path(), &homma_core::repo::GixGit);
    assert_eq!(
        cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["one"]
    );
    cfg.detect_members(second.path(), &homma_core::repo::GixGit);
    assert_eq!(
        cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["two"]
    );
}

#[test]
fn a_root_that_is_not_there_is_a_workspace_with_no_members() {
    // Rather than a panic. A manifest can name a workspace path that has not
    // been created yet, and reporting nothing is the honest answer.
    let mut cfg = config();
    cfg.detect_members(
        Path::new("/nonexistent/nowhere/at/all"),
        &homma_core::repo::GixGit,
    );
    assert!(cfg.repos.is_empty());
}

#[test]
fn the_detection_can_actually_fail() {
    // Every case above asserts what detection found. A detector that found
    // nothing would satisfy the exclusions perfectly and fail only the
    // inclusions, so this is the one that says the exclusions mean something:
    // the same tree, with the exclusions turned into real clones, is detected
    // in full.
    let root = tempfile::tempdir().unwrap();
    for name in ["notes", "scripts", "seat"] {
        clone_at(root.path(), name, None);
    }
    let mut cfg = config();
    cfg.detect_members(root.path(), &homma_core::repo::GixGit);
    assert_eq!(
        cfg.repos.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["notes", "scripts", "seat"],
        "the walk misses directories it should reach"
    );
}

#[test]
fn the_root_walked_is_the_workspace_root_and_not_the_manifest_s_own_directory() {
    // The two are the same whenever `workspace.path` is left at `.`, which is
    // every ordinary workspace, so nothing here notices when they diverge. A
    // manifest that does set the path is the case that does: detecting beside
    // the manifest while every consumer resolves against the root would report
    // a member whose local path points at a directory nobody looked in.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("elsewhere");
    std::fs::create_dir_all(&root).unwrap();
    clone_at(
        &root,
        "under-the-root",
        Some("https://github.com/orgrinrt/x.git"),
    );
    clone_at(dir.path(), "beside-the-manifest", None);

    let manifest = dir.path().join("homma.toml");
    std::fs::write(
        &manifest,
        format!(
            "[workspace]\nname = \"demo\"\npath = \"{}\"\n",
            root.display()
        ),
    )
    .unwrap();

    let cfg = Config::from_path(&manifest).expect("the manifest parses");
    assert!(
        cfg.repo("under-the-root").is_some(),
        "a clone under the workspace root is a member: {:?}",
        cfg.repos.keys().collect::<Vec<_>>()
    );
    assert!(
        cfg.repo("beside-the-manifest").is_none(),
        "a clone beside the manifest but outside the root is not"
    );
}
