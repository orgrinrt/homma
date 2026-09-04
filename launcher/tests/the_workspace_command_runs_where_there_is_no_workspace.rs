//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma workspace`, against real repositories in temporary directories.
//!
//! Every property here is a property of git's own behaviour or of a directory
//! on disk, so a mock would prove nothing: the checks the spawn scripts' suite
//! made by hand are made here against the same things. A content repository
//! and a member are created locally and cloned by path, which is what a url
//! is to git, so nothing reaches the network.
//!
//! A test about this repository rather than about the package, so it is not
//! in `include`.

use std::path::{Path, PathBuf};
use std::process::Command;

use homma::settings::{CONTENT_REPO, Prefs};
use homma::workspace::{Ask, answer, spawn, status};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository with one commit on `dev`, at `dir`.
fn repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "dev"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("README.md"), "first\n").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-qm", "first"]);
}

/// The same, on `main` only, for the trunk rule's other arm.
fn repo_on_main(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("README.md"), "first\n").unwrap();
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-qm", "first"]);
}

struct Fixture {
    _tmp:    tempfile::TempDir,
    home:    PathBuf,
    content: PathBuf,
    member:  PathBuf,
    prefs:   Prefs,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let content = base.join("remotes").join("ws.git");
    let member = base.join("remotes").join("notko.git");
    repo(&content);
    repo_on_main(&member);
    let prefs = Prefs {
        disallowed_roots: vec!["~".into()],
        workspaces_root:  home.join("work"),
        content_repo:     content.to_string_lossy().into_owned(),
        repos:            vec![member.to_string_lossy().into_owned()],
    };
    Fixture {
        _tmp: tmp,
        home,
        content,
        member,
        prefs,
    }
}

fn run(f: &Fixture, cwd: &Path, root: Option<&Path>, ask: Ask) -> Result<String, String> {
    let mut out = Vec::new();
    answer(&f.prefs, Some(&f.home), cwd, root, ask, &mut out)?;
    Ok(String::from_utf8(out).unwrap())
}

#[test]
fn the_home_directory_is_refused_by_default_and_a_subtree_only_when_asked() {
    let f = fixture();
    let home = &f.home;
    let refused = |p: &Prefs, d: &Path| p.refusal_for(d, Some(home)).unwrap().is_some();
    // the default: the home itself, spelled any way, and nothing under it
    assert!(refused(&f.prefs, home));
    assert!(refused(&f.prefs, &home.join(".")));
    assert!(refused(&f.prefs, &home.join("x").join("..")));
    assert!(!refused(&f.prefs, &home.join("work").join("alpha")));
    assert!(!refused(&f.prefs, &home.join("work")));
    // a link to the home is the home
    let link = home.parent().unwrap().join("h-link");
    std::os::unix::fs::symlink(home, &link).unwrap();
    assert!(refused(&f.prefs, &link));
    // the subtree form denies everything under it and the directory itself
    let subtree = Prefs {
        disallowed_roots: vec!["~/work/*".into()],
        ..f.prefs.clone()
    };
    assert!(refused(&subtree, &home.join("work").join("alpha")));
    assert!(refused(&subtree, &home.join("work").join("a").join("b")));
    assert!(refused(&subtree, &home.join("work")));
    assert!(!refused(&subtree, &home.join("other")));
    assert!(
        !refused(&subtree, home),
        "the subtree form is not the plain form"
    );
    // and the refusal names the key, so the reader knows where to look
    let why = f.prefs.refusal_for(home, Some(home)).unwrap().unwrap();
    assert!(why.contains("disallowed_roots"), "{why}");
    // a plain absolute entry, and an empty list denying nothing
    let abs = Prefs {
        disallowed_roots: vec![home.join("work").to_string_lossy().into_owned()],
        ..f.prefs.clone()
    };
    assert!(refused(&abs, &home.join("work")));
    assert!(!refused(&abs, home));
    let none = Prefs {
        disallowed_roots: vec![],
        ..f.prefs.clone()
    };
    assert!(!refused(&none, home));
}

#[test]
fn spawn_by_slug_clones_the_content_repository_and_the_members_on_their_trunks() {
    let f = fixture();
    let said = run(&f, &f.home, None, Ask::Spawn {
        slug:   "alpha".into(),
        repos:  vec![],
        branch: Some("feat/x".into()),
    })
    .unwrap();
    let ws = f.home.join("work").join("alpha");
    assert!(ws.join(".git").is_dir(), "{said}");
    assert!(ws.join("notko").join(".git").is_dir(), "{said}");
    // the content repository has a `dev`, the member has only `main`, and
    // each was cloned on its own
    let members = status::survey(&ws).unwrap();
    let names: Vec<_> = members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec![".", "notko"]);
    assert_eq!(members[0].branch, "dev");
    // `--branch` switched the member, not the workspace's own clone
    assert_eq!(members[1].branch, "feat/x");
    assert!(members.iter().all(|m| !m.holds_work()), "{members:?}");
    assert!(said.contains("work here:"), "{said}");
    assert!(said.contains("notko  main -> feat/x"), "{said}");

    // the same slug again is refused rather than cloned over
    let err = run(&f, &f.home, None, Ask::Spawn {
        slug:   "alpha".into(),
        repos:  vec![],
        branch: None,
    })
    .expect_err("cloned over a workspace");
    assert!(err.contains("already exists"), "{err}");

    // list sees it
    let listed = run(&f, &f.home, None, Ask::List).unwrap();
    assert_eq!(listed, "alpha\n");

    // an extra repository on the command line, by url, lands beside
    let second = f.home.join("remotes2").join("arvo.git");
    repo(&second);
    let said = run(&f, &f.home, None, Ask::Spawn {
        slug:   "beta".into(),
        repos:  vec![second.to_string_lossy().into_owned()],
        branch: None,
    })
    .unwrap();
    let beta = f.home.join("work").join("beta");
    assert!(beta.join("arvo").join(".git").is_dir(), "{said}");
    assert!(beta.join("notko").join(".git").is_dir(), "{said}");
    assert_eq!(run(&f, &f.home, None, Ask::List).unwrap(), "alpha\nbeta\n");
}

#[test]
fn spawn_refuses_before_touching_the_disk_when_it_cannot_go_ahead() {
    let f = fixture();
    // no content repository named: the key and the command that sets it
    let empty = Prefs {
        content_repo: String::new(),
        ..f.prefs.clone()
    };
    let mut out = Vec::new();
    let err = answer(
        &empty,
        Some(&f.home),
        &f.home,
        None,
        Ask::Spawn {
            slug:   "alpha".into(),
            repos:  vec![],
            branch: None,
        },
        &mut out,
    )
    .expect_err("spawned from nowhere");
    assert!(
        err.contains(CONTENT_REPO) && err.contains("homma config set"),
        "{err}"
    );
    assert!(
        !f.home.join("work").exists(),
        "the workspaces root was made for nothing"
    );

    // a destination under a disallowed root
    let denied = Prefs {
        disallowed_roots: vec!["~/work/*".into()],
        ..f.prefs.clone()
    };
    let err = answer(
        &denied,
        Some(&f.home),
        &f.home,
        None,
        Ask::Spawn {
            slug:   "alpha".into(),
            repos:  vec![],
            branch: None,
        },
        &mut out,
    )
    .expect_err("spawned under a disallowed root");
    assert!(err.contains("disallowed_roots"), "{err}");
    assert!(!f.home.join("work").exists());
}

#[test]
fn spawn_in_place_takes_an_empty_directory_outside_any_repository() {
    let f = fixture();
    let cwd = f.home.join("here");
    std::fs::create_dir_all(&cwd).unwrap();
    let said = run(&f, &cwd, None, Ask::Bare).unwrap();
    assert!(cwd.join(".git").is_dir(), "{said}");
    assert!(cwd.join("notko").join(".git").is_dir(), "{said}");
    assert!(
        said.starts_with(&format!("spawning {}\n", cwd.display())),
        "{said}"
    );

    // not empty: refused, and what was there is untouched
    let full = f.home.join("full");
    std::fs::create_dir_all(&full).unwrap();
    std::fs::write(full.join("note"), "mine").unwrap();
    let err = run(&f, &full, None, Ask::Bare).expect_err("spawned over a file");
    assert!(err.contains("not empty"), "{err}");
    assert_eq!(std::fs::read_to_string(full.join("note")).unwrap(), "mine");

    // inside a repository: refused, naming it, even when the directory is empty
    let inside = f.content.join("sub");
    std::fs::create_dir_all(&inside).unwrap();
    let err = run(&f, &inside, None, Ask::Bare).expect_err("spawned inside a repository");
    assert!(err.contains("inside the repository at"), "{err}");
    assert!(!inside.join(".git").exists());

    // the home itself: the default refusal, in place
    let err = run(&f, &f.home, None, Ask::Bare).expect_err("spawned into the home");
    assert!(err.contains("disallowed_roots"), "{err}");
    assert_eq!(spawn::repository_above(&f.home), None);
}

#[test]
fn status_reports_and_reap_refuses_on_exactly_what_would_be_lost() {
    let f = fixture();
    run(&f, &f.home, None, Ask::Spawn {
        slug:   "alpha".into(),
        repos:  vec![],
        branch: None,
    })
    .unwrap();
    let ws = f.home.join("work").join("alpha");

    // bare, inside: the status, with the root found by the caller
    let said = run(&f, &ws.join("notko"), Some(&ws), Ask::Bare).unwrap();
    assert!(
        said.starts_with(&format!("workspace {}\n", ws.display())),
        "{said}"
    );
    assert!(said.contains("  .  dev  "), "{said}");
    assert!(said.contains("  notko  main  "), "{said}");
    assert!(said.contains("clean"), "{said}");

    // a dirty member refuses the reap
    std::fs::write(ws.join("notko").join("draft"), "half").unwrap();
    let err = run(&f, &ws, Some(&ws), Ask::Reap {
        slug:  None,
        force: false,
    })
    .expect_err("reaped over a dirty tree");
    assert!(
        err.contains("notko: uncommitted or untracked changes"),
        "{err}"
    );
    assert!(ws.exists());
    let said = run(&f, &ws, Some(&ws), Ask::Bare).unwrap();
    assert!(said.contains("notko  main"), "{said}");
    assert!(said.contains("dirty"), "{said}");

    // committed, it is a commit on no remote, which refuses too and is listed
    git(&ws.join("notko"), &["add", "draft"]);
    git(&ws.join("notko"), &[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=t",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "the draft",
    ]);
    let err = run(&f, &f.home, None, Ask::Reap {
        slug:  Some("alpha".into()),
        force: false,
    })
    .expect_err("reaped over an unpushed commit");
    assert!(err.contains("notko: 1 commit(s) on no remote"), "{err}");
    assert!(err.contains("the draft"), "{err}");
    assert!(ws.exists());

    // pushed, nothing is held and the reap goes through
    git(&ws.join("notko"), &[
        "push",
        "-q",
        "origin",
        "HEAD:refs/heads/draft",
    ]);
    let said = run(&f, &f.home, None, Ask::Reap {
        slug:  Some("alpha".into()),
        force: false,
    })
    .unwrap();
    assert!(said.contains("removed"), "{said}");
    assert!(!ws.exists());
    // and the member's remote is where the commit went, which is the point
    let branches = Command::new("git")
        .args(["branch", "--list", "draft"])
        .current_dir(&f.member)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).contains("draft"));

    // --force removes a held workspace, having said what it discards
    run(&f, &f.home, None, Ask::Spawn {
        slug:   "gamma".into(),
        repos:  vec![],
        branch: None,
    })
    .unwrap();
    let gamma = f.home.join("work").join("gamma");
    std::fs::write(gamma.join("stray"), "x").unwrap();
    let said = run(&f, &f.home, None, Ask::Reap {
        slug:  Some("gamma".into()),
        force: true,
    })
    .unwrap();
    assert!(
        said.contains("--force given") && said.contains(".: uncommitted"),
        "{said}"
    );
    assert!(!gamma.exists());

    // a slug that is not there
    let err = run(&f, &f.home, None, Ask::Reap {
        slug:  Some("nope".into()),
        force: false,
    })
    .expect_err("reaped nothing");
    assert!(err.contains("no workspace at"), "{err}");
    // and reaping the current one from outside any needs a slug
    let err = run(&f, &f.home, None, Ask::Reap {
        slug:  None,
        force: false,
    })
    .expect_err("reaped the void");
    assert!(err.contains("needs a slug"), "{err}");
}

#[test]
fn the_binary_answers_workspace_and_config_with_no_manifest_above_the_cwd() {
    // Through the installed shape: the launcher's dispatch, the settings read
    // off the environment, and no engine, no root and no network.
    let f = fixture();
    let cfg_dir = f.home.join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let homma = |cwd: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_homma"))
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &f.home)
            .env("HOMMA_CONFIG", &cfg_dir)
            .env("HOMMA_NO_SELF_UPDATE", "1")
            .env("HOMMA_CFG_SPAWN_CONTENT_REPO", &f.prefs.content_repo)
            .env("HOMMA_CFG_WORKSPACES_ROOT", f.home.join("work"))
            .output()
            .unwrap()
    };
    let out = homma(&f.home, &["workspace", "list"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("no workspaces"));

    let out = homma(&f.home, &["workspace", "spawn", "alpha"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.home.join("work").join("alpha").join(".git").is_dir());

    // a flag after the name is the launcher's and reaches the setting
    let out = homma(&f.home, &[
        "workspace",
        "list",
        "--cfg",
        "workspaces_root=/nowhere/at/all",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("/nowhere/at/all"));

    // the schema names the four keys, user scope, with the defaults
    let out = homma(&f.home, &["config", "schema"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let schema = String::from_utf8_lossy(&out.stdout);
    for key in ["disallowed_roots", "workspaces_root", "spawn.content_repo", "spawn.repos"] {
        assert!(schema.contains(&format!("{key}\t")), "{schema}");
    }
    assert!(schema.contains("[\"~\"]"), "{schema}");
    assert_eq!(schema.matches("\tuser\t").count(), 4, "{schema}");

    // a refusal is under the tool's name on stderr and nonzero
    let out = homma(&f.home, &["workspace", "reap", "nope"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("homma: no workspace at"), "{err}");

    // and the home itself is refused in place, through the default
    let out = homma(&f.home, &["workspace"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("disallowed_roots"));
}
