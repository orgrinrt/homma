//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke tests for `homma release`, through the binary, off the network.

mod support;

use predicates::prelude::*;
use support::{bin, clone_at, committed_crate, git_in, minimal_config_toml};

#[test]
fn release_is_listed_and_plan_prints_the_next_version_without_moving_anything() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    bin()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("release"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "minor"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "version 0.1.0 becomes 0.2.0, tagged `v0.2.0`",
        ))
        .stdout(predicate::str::contains("feat: first"))
        .stdout(predicate::str::contains("x to crates-io"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--level"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "huge"])
        .assert()
        .failure();
    assert!(!dir.path().join("x/CHANGELOG.md").exists());
}

#[test]
fn a_gate_on_a_commit_the_repo_does_not_have_is_refused_before_anything_runs() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    // the refusal comes from resolving the commit, ahead of the gate's own
    // steps, so nothing is built and no record is written
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "gate", "x", "--sha", "deadbeef"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("`deadbeef` is not a commit"));
    assert!(!dir.path().join(".data/homma").exists());
}

#[test]
fn a_post_names_its_commit_in_full_whatever_was_typed() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    let out = std::process::Command::new("git")
        .args(["-C", dir.path().join("x").to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    let full = String::from_utf8(out.stdout).unwrap().trim().to_string();
    // a string that is no commit is refused as one, ahead of the store
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "gate", "x", "--post", "deadbeef"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("`deadbeef` is not a commit"));
    // a short sha of a real commit is looked up under its full one, which
    // is what the not-found line carries
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "gate", "x", "--post", &full[.. 7]])
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "no gate run recorded on {full}"
        )));
}

#[test]
fn a_workspace_wide_run_names_each_repo_it_passes_over_and_why() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    // `x` is released at its tip, `y` has no manifest at all; neither is
    // released and both are named, so the sweep never reaches a registry
    committed_crate(dir.path(), "x");
    git_in(&dir.path().join("x"), &["config", "tag.gpgsign", "false"]);
    git_in(&dir.path().join("x"), &[
        "tag", "-a", "v0.1.0", "-m", "v0.1.0",
    ]);
    clone_at(dir.path(), "y", Some("https://github.com/orgrinrt/y.git"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "run", "--level", "patch", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "x: nothing unreleased, passed over",
        ))
        .stdout(predicate::str::contains("y: passed over"));
    assert!(!dir.path().join("x/CHANGELOG.md").exists());
}

#[test]
fn a_workspace_wide_run_refuses_a_manifest_off_the_level_like_a_single_run() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    // tagged at 0.1.0, manifest bumped to 0.2.0, asked for a patch
    committed_crate(dir.path(), "z");
    let z = dir.path().join("z");
    git_in(&z, &["config", "tag.gpgsign", "false"]);
    git_in(&z, &["tag", "-a", "v0.1.0", "-m", "v0.1.0"]);
    std::fs::write(
        z.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.2.0\"\n",
    )
    .unwrap();
    git_in(&z, &["commit", "-qam", "chore: bump"]);
    // the sweep refuses in its pre-pass, before any registry is asked; the
    // single-repo run asks the registries first and its refusal is covered
    // where `plan` is tested, off the network
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "run", "--level", "patch", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("z:"))
        .stderr(predicate::str::contains("0.2.0"))
        .stderr(predicate::str::contains("0.1.1"));
    assert!(!z.join("CHANGELOG.md").exists());
}

#[test]
fn hook_install_writes_one_entrypoint_per_event_and_says_how_git_reaches_them() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "{}\n[[hooks.pre-commit]]\nrun = \"echo checked {{paths}}\"\npaths = [\"*.md\"]\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    committed_crate(dir.path(), "x");
    bin()
        .args(["-c", cfg.to_str().unwrap(), "hook", "install", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hooks/pre-push"))
        .stdout(predicate::str::contains("hooks/pre-commit"))
        .stdout(predicate::str::contains(
            "reads the hooks directory directly",
        ));
    let script = std::fs::read_to_string(dir.path().join("x/.git/hooks/pre-push")).unwrap();
    assert!(script.contains("homma hook run pre-push \"$@\""));
    let script = std::fs::read_to_string(dir.path().join("x/.git/hooks/pre-commit")).unwrap();
    assert!(script.contains("homma hook run pre-commit \"$@\""));
    assert!(
        !dir.path().join("x/.git/hooks/commit-msg").exists(),
        "no entries, no entrypoint"
    );
    // and the entrypoint's verb runs the table: nothing staged, so the
    // markdown entry is skipped and the event passes
    bin()
        .args(["-c", cfg.to_str().unwrap(), "hook", "run", "pre-commit"])
        .current_dir(dir.path().join("x"))
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("no path matches"));
    // an event with no entries passes and says so
    bin()
        .args(["-c", cfg.to_str().unwrap(), "hook", "run", "commit-msg", "MSG"])
        .current_dir(dir.path().join("x"))
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("no entries"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "hook", "install", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("`nope` is not a repository"));
    // a table naming an event git has no hook for refuses the manifest
    std::fs::write(
        &cfg,
        format!(
            "{}\n[[hooks.pre-comit]]\nrun = \"x\"\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    bin()
        .args(["-c", cfg.to_str().unwrap(), "hook", "install", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`[hooks.pre-comit]` names no git hook",
        ));
}

#[test]
fn a_worktree_beside_the_clones_resolves_to_its_clone_with_no_repo_named() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    // a worktree under `.worktrees/`, which is where every agent's seat
    // lives and where a pre-push hook runs from
    let seat = dir.path().join(".worktrees/seat");
    std::fs::create_dir_all(dir.path().join(".worktrees")).unwrap();
    git_in(&dir.path().join("x"), &[
        "worktree",
        "add",
        "-q",
        seat.to_str().unwrap(),
        "-b",
        "topic",
    ]);
    bin()
        .current_dir(&seat)
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "--level", "patch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("version 0.1.0 becomes 0.1.1"));
    // the control: a directory that is no worktree is still refused
    let stray = dir.path().join("stray");
    std::fs::create_dir_all(&stray).unwrap();
    bin()
        .current_dir(&stray)
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "--level", "patch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "not inside a workspace repository",
        ));
    // the hook's own invocation, git's two arguments and the refs on stdin,
    // from the worktree: with no ref pushed there is nothing to gate, which
    // is reached only after the repo resolved and the arguments parsed
    bin()
        .current_dir(&seat)
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "release",
            "gate",
            "--hook",
            "origin",
            "git@github.com:orgrinrt/x.git",
        ])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to gate"));
    // and under the hook the positional is git's remote name, not a repo:
    // `x` is a real repo here, and from a stray directory it is still not
    // read as one
    bin()
        .current_dir(&stray)
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "release",
            "gate",
            "--hook",
            "x",
            "git@github.com:orgrinrt/x.git",
        ])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "not inside a workspace repository",
        ));
}

#[test]
fn the_install_writes_into_gits_own_directory_and_says_where_the_hooks_path_leads() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    let x = dir.path().join("x");
    let install = || {
        bin()
            .args(["-c", cfg.to_str().unwrap(), "hook", "install", "x"])
            .assert()
    };
    // a hooks path with nothing under it for the event: written, since git's
    // own directory is where a chain would reach, and reported as not
    // reached, a line and a non-zero exit so a sweep goes on
    let elsewhere = tempfile::tempdir().unwrap();
    git_in(&x, &[
        "config",
        "core.hooksPath",
        elsewhere.path().to_str().unwrap(),
    ]);
    install()
        .failure()
        .stdout(predicate::str::contains("hooks/pre-push"))
        .stdout(predicate::str::contains("nothing at"))
        .stdout(predicate::str::contains("will not run"));
    assert!(
        !elsewhere.path().join("pre-push").exists(),
        "never written where the path points"
    );
    assert!(x.join(".git/hooks/pre-push").exists());
    // another tool's hook under the path: written, and said not to be reached
    std::fs::write(
        elsewhere.path().join("pre-push"),
        "#!/bin/sh\necho theirs\n",
    )
    .unwrap();
    install()
        .failure()
        .stdout(predicate::str::contains("not mockspace's"));
    // mockspace's hook under the path, read off its own first lines: reached
    // through its chain, and said so; the path's spelling says nothing
    std::fs::write(
        elsewhere.path().join("pre-push"),
        "#!/usr/bin/env bash\n# mockspace-managed v3 fp:0\n# mockspace durable gate (pre-push)\nexit 0\n",
    )
    .unwrap();
    install()
        .success()
        .stdout(predicate::str::contains("chains through"))
        .stdout(predicate::str::contains("after its own checks"));
    // a hooks directory the repo tracks is not where anything is written,
    // so it changes nothing about the install
    std::fs::create_dir_all(x.join(".githooks")).unwrap();
    std::fs::write(x.join(".githooks/pre-commit"), "#!/bin/sh\n").unwrap();
    git_in(&x, &["config", "core.hooksPath", ".githooks"]);
    git_in(&x, &["add", ".githooks"]);
    git_in(&x, &["commit", "-qm", "chore: hooks"]);
    install()
        .failure()
        .stdout(predicate::str::contains("nothing at"));
    assert!(!x.join(".githooks/pre-push").exists());
    // a pre-push already there that is not homma's is refused whatever the path
    git_in(&x, &["config", "--unset", "core.hooksPath"]);
    std::fs::write(x.join(".git/hooks/pre-push"), "#!/bin/sh\necho mine\n").unwrap();
    install()
        .failure()
        .stdout(predicate::str::contains("not homma's"));
    assert_eq!(
        std::fs::read_to_string(x.join(".git/hooks/pre-push")).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
}

#[test]
fn release_badges_reads_the_newest_run_whichever_branch_it_measured() {
    use homma_api::{GateRun, Step, StepOutcome, Verdict};
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    let repo = dir.path().join("x");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    // a `dev`/`main` repo whose trunk is one commit past the release line,
    // pushing to a bare origin so the badges branch has somewhere to land
    let bare = dir.path().join("origin.git");
    std::process::Command::new("git")
        .args(["init", "-q", "--bare", bare.to_str().unwrap()])
        .status()
        .unwrap();
    git(&["remote", "set-url", "origin", bare.to_str().unwrap()]);
    git(&["branch", "-M", "main"]);
    git(&["switch", "-qc", "dev"]);
    std::fs::write(repo.join("src.rs"), "fn f() {}\n").unwrap();
    git(&["add", "src.rs"]);
    git(&["commit", "-qm", "feat: more"]);
    let dev_tip = git(&["rev-parse", "dev"]).trim().to_string();
    // with nothing recorded the command says so and writes nothing
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "badges", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no gate run recorded for `x`"));
    // a green run on the trunk's tip, which is where the hook records them
    let store = homma_store::Store::open(dir.path().join(".data/homma"));
    let mut step = StepOutcome::skipped(Step::Tests);
    step.skipped = false;
    step.passed = true;
    let run = GateRun {
        repo:    "x".into(),
        sha:     dev_tip.clone(),
        ran_at:  "2026-09-02T22:00:00Z".into(),
        verdict: Verdict::Green,
        steps:   vec![step],
    };
    store.append(&GateRun::kind(), &run.to_record()).unwrap();
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "badges", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote"))
        .stdout(predicate::str::contains(format!(
            "from the run on {}",
            &dev_tip[.. 7]
        )));
    let on_origin = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "branch", "--list", "badges"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&on_origin.stdout).contains("badges"));
    let gate = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "show", "badges:gate.json"])
        .output()
        .unwrap();
    assert!(gate.status.success(), "the badges branch carries gate.json");
}

#[test]
fn a_fresh_clone_with_only_a_remote_dev_still_merges_dev_onto_main() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    // an upstream with `main` and `dev`, cloned fresh: the clone holds
    // `origin/dev` and no local `dev`, which is the ordinary state
    committed_crate(dir.path(), "up");
    let up = dir.path().join("up");
    git_in(&up, &["branch", "-M", "main"]);
    git_in(&up, &["branch", "dev"]);
    let fresh = dir.path().join("x");
    let out = std::process::Command::new("git")
        .args(["clone", "-q", up.to_str().unwrap(), fresh.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let branches = std::process::Command::new("git")
        .args(["-C", fresh.to_str().unwrap(), "branch", "--list"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&branches.stdout).contains("dev"),
        "the control: no local dev"
    );
    // `dev` is the branch, and the clone is told to check it out rather than
    // being released off `main` as though there were no `dev` at all
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "patch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("git switch dev"));
    git_in(&fresh, &["switch", "-q", "dev"]);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "patch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("`dev` onto `main`"));
    // and a repo with no dev anywhere is named as `main` alone
    git_in(&fresh, &["switch", "-q", "main"]);
    git_in(&fresh, &["branch", "-qD", "dev"]);
    git_in(&fresh, &["push", "-q", "origin", "--delete", "dev"]);
    git_in(&fresh, &["fetch", "-q", "--prune"]);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "patch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("`main` alone"));
}
