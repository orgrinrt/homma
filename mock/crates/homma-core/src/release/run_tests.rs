//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release against a real repository with a bare remote, a runner that
//! records instead of publishing, and a forge that records instead of posting.

use std::cell::RefCell;

use homma_api::{Step, StepOutcome, Version};

use super::*;
use crate::forge::{CommitStatus, CreateRepoSpec, RepoMetadata};
use crate::release::sh;

struct Fake(RefCell<Vec<String>>);

impl Runner for Fake {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        self.0
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        Ok(sh::Output {
            program: program.into(),
            args:    args.iter().map(|a| a.to_string()).collect(),
            status:  Some(0),
            stdout:  String::new(),
            stderr:  String::new(),
        })
    }
}

#[derive(Default)]
struct Recorder {
    releases: RefCell<Vec<(String, String)>>,
}

impl Forge for Recorder {
    fn fetch_repo(&self, _: &str, _: &str) -> Result<RepoMetadata, ForgeError> {
        unreachable!()
    }

    fn repo_exists(&self, _: &str, _: &str) -> Result<bool, ForgeError> {
        unreachable!()
    }

    fn create_repo(&self, _: &str, _: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
        unreachable!()
    }

    fn archive_repo(&self, _: &str, _: &str) -> Result<(), ForgeError> {
        unreachable!()
    }

    fn delete_repo(&self, _: &str, _: &str) -> Result<(), ForgeError> {
        unreachable!()
    }

    fn credential_works(&self) -> Result<bool, ForgeError> {
        Ok(true)
    }

    fn set_commit_status(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &CommitStatus,
    ) -> Result<(), ForgeError> {
        Ok(())
    }

    fn create_release(&self, _: &str, _: &str, tag: &str, body: &str) -> Result<(), ForgeError> {
        self.releases.borrow_mut().push((tag.into(), body.into()));
        Ok(())
    }
}

struct Fixture {
    work:  tempfile::TempDir,
    _bare: tempfile::TempDir,
}

impl Fixture {
    fn root(&self) -> &Path {
        self.work.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let out = sh::run(self.root(), "git", args).unwrap();
        assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
        out.stdout.trim().to_string()
    }

    /// A crate at 0.1.0, tagged and released, with one feat commit on `dev`
    /// since, everything pushed, the tree on `dev`.
    fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        let bare = tempfile::tempdir().unwrap();
        assert!(
            sh::run(bare.path(), "git", &["init", "--quiet", "--bare"])
                .unwrap()
                .ok()
        );
        let f = Fixture {
            work,
            _bare: bare,
        };
        f.git(&["init", "--quiet", "-b", "main"]);
        f.git(&["config", "user.email", "t@t"]);
        f.git(&["config", "user.name", "t"]);
        f.git(&["config", "tag.gpgsign", "false"]);
        f.git(&["config", "commit.gpgsign", "false"]);
        f.git(&["remote", "add", "origin", f._bare.path().to_str().unwrap()]);
        std::fs::write(
            f.root().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        f.git(&["add", "."]);
        f.git(&["commit", "--quiet", "-m", "feat: first"]);
        f.git(&["tag", "-a", "v0.1.0", "-m", "v0.1.0"]);
        f.git(&["switch", "--quiet", "-c", "dev"]);
        std::fs::write(f.root().join("lib.rs"), "// x").unwrap();
        f.git(&["add", "."]);
        f.git(&["commit", "--quiet", "-m", "feat: the thing"]);
        f.git(&["push", "--quiet", "origin", "main", "dev", "refs/tags/v0.1.0"]);
        f
    }
}

fn green_run(sha: &str) -> GateRun {
    let mut tests = StepOutcome::skipped(Step::Tests);
    tests.skipped = false;
    tests.numbers.insert("tests".into(), "3".into());
    tests.numbers.insert("passed".into(), "3".into());
    GateRun {
        repo:    "x".into(),
        sha:     sha.into(),
        ran_at:  "t".into(),
        verdict: Verdict::Green,
        steps:   vec![tests],
    }
}

fn published() -> Published {
    let mut p = Published::default();
    p.versions
        .insert((Registry::CratesIo, "x".into()), vec![Version::new(
            0, 1, 0,
        )]);
    p
}

fn token(r: Registry) -> Result<String, String> {
    Ok(format!("t-{r}"))
}

fn served(
    _: Registry,
    _: &str,
    _: &Version,
) -> Result<bool, crate::release::registry::Unreachable> {
    Ok(true)
}

fn setup<'a>(
    runner: &'a Fake,
    forge: &'a Recorder,
    published: &'a Published,
    trunk: &'a str,
) -> Setup<'a> {
    Setup {
        runner,
        forge,
        owner: "o",
        name: "x",
        remote: "origin",
        trunk,
        release: "main",
        date: "2026-09-02",
        token: &token,
        served: &served,
        published,
    }
}

#[test]
fn a_blocking_finding_refuses_before_anything_runs() {
    let f = Fixture::new();
    std::fs::write(f.root().join("stray"), "s").unwrap();
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let err = release(
        &setup(&runner, &forge, &p, "dev"),
        f.root(),
        Level::Patch,
        None,
        false,
    )
    .unwrap_err();
    match err {
        ReleaseError::Blocked(findings) => {
            assert!(findings.iter().any(|x| x.id == "tree.untracked"))
        },
        other => panic!("{other:?}"),
    }
    assert!(runner.0.borrow().is_empty());
    assert!(forge.releases.borrow().is_empty());
}

#[test]
fn no_run_or_a_red_run_or_a_run_on_another_sha_refuses() {
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    assert!(matches!(
        step_gate_run(f.root(), "dev", None),
        Err(ReleaseError::NoGreenRun {
            found: None,
            ..
        })
    ));
    let mut red = green_run(&tip);
    red.verdict = Verdict::Red;
    assert!(matches!(
        step_gate_run(f.root(), "dev", Some(&red)),
        Err(ReleaseError::NoGreenRun {
            found: Some(Verdict::Red),
            ..
        })
    ));
    let elsewhere = green_run("0000000");
    assert!(matches!(
        step_gate_run(f.root(), "dev", Some(&elsewhere)),
        Err(ReleaseError::NoGreenRun {
            found: None,
            ..
        })
    ));
    step_gate_run(f.root(), "dev", Some(&green_run(&tip))).unwrap();
}

#[test]
fn a_dry_run_returns_the_plan_and_moves_nothing() {
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let out = release(
        &setup(&runner, &forge, &p, "dev"),
        f.root(),
        Level::Minor,
        Some(&green_run(&tip)),
        true,
    )
    .unwrap();
    let plan = out.unwrap_err();
    assert_eq!(plan.next, Version::new(0, 2, 0));
    assert_eq!(plan.commits.len(), 1);
    assert_eq!(f.git(&["rev-parse", "dev"]), tip);
    assert!(f.git(&["tag", "--list"]) == "v0.1.0");
    assert!(runner.0.borrow().is_empty());
    assert!(!f.root().join("CHANGELOG.md").exists());
}

#[test]
fn the_whole_release_bumps_merges_tags_releases_publishes_and_writes_badges() {
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let done = release(
        &setup(&runner, &forge, &p, "dev"),
        f.root(),
        Level::Patch,
        Some(&green_run(&tip)),
        false,
    )
    .unwrap()
    .unwrap();
    // the bump commit on dev carries the manifest and the changelog
    assert_eq!(
        f.git(&["log", "-1", "--format=%s", "dev"]),
        "chore: release 0.1.1"
    );
    let changed = f.git(&["show", "--name-only", "--format=", &done.bump_sha]);
    assert!(
        changed.contains("Cargo.toml") && changed.contains("CHANGELOG.md"),
        "{changed}"
    );
    let manifest = f.git(&["show", "dev:Cargo.toml"]);
    assert!(manifest.contains("version = \"0.1.1\""));
    let log = f.git(&["show", "dev:CHANGELOG.md"]);
    assert!(
        log.starts_with("# Changelog\n\n## 0.1.1 (2026-09-02)\n"),
        "{log}"
    );
    assert!(log.contains("feat: the thing"));
    // main got a merge commit and the tag sits on it, annotated
    assert_eq!(f.git(&["rev-parse", "main"]), done.tag_sha);
    assert_eq!(git::parent_count(f.root(), "main").unwrap(), 2);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert!(git::tag_is_annotated(f.root(), "v0.1.1").unwrap());
    // everything is on the remote
    assert_eq!(f.git(&["rev-parse", "origin/main"]), done.tag_sha);
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), done.bump_sha);
    assert!(
        git::remote_tags(f.root(), "origin")
            .unwrap()
            .iter()
            .any(|(n, s)| n == "v0.1.1" && *s == done.tag_sha)
    );
    assert_eq!(f.git(&["rev-parse", "origin/badges"]), done.badges_sha);
    assert_eq!(git::parent_count(f.root(), "badges").unwrap(), 0);
    let on = git::files_on(f.root(), "badges").unwrap();
    assert!(on.iter().any(|(n, _)| n == "version.json"));
    assert!(
        on.iter()
            .any(|(n, body)| n == "tests.json" && body.contains("3 of 3"))
    );
    // the forge release carries the block, and the publish ran once
    let releases = forge.releases.borrow();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].0, "v0.1.1");
    assert!(releases[0].1.starts_with("## 0.1.1 (2026-09-02)"));
    let ran = runner.0.borrow();
    assert!(
        ran.iter().any(|l| l == "cargo publish -p x --locked"),
        "{ran:?}"
    );
    // and the tree is back on the trunk, clean
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_repo_released_from_its_trunk_tags_the_bump_commit_and_merges_nothing() {
    let f = Fixture::new();
    f.git(&["switch", "--quiet", "main"]);
    f.git(&["merge", "--quiet", "--ff-only", "dev"]);
    f.git(&["push", "--quiet", "origin", "main"]);
    let tip = f.git(&["rev-parse", "main"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let mut s = setup(&runner, &forge, &p, "main");
    s.release = "main";
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, done.tag_sha);
    assert_eq!(git::parent_count(f.root(), "main").unwrap(), 1);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.bump_sha);
}

#[test]
fn the_bump_refuses_off_the_trunk() {
    let f = Fixture::new();
    f.git(&["switch", "--quiet", "main"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let plan = plan::plan(f.root(), "dev", Level::Patch, "d").unwrap();
    assert!(matches!(
        step_bump(&setup(&runner, &forge, &p, "dev"), f.root(), &plan),
        Err(ReleaseError::NotOnTrunk(Some(b))) if b == "main"
    ));
}

#[test]
fn a_push_that_fails_mid_release_hands_the_tree_back_to_the_trunk_clean() {
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let plan = plan::plan(f.root(), "dev", Level::Patch, "d").unwrap();
    let s = setup(&runner, &forge, &p, "dev");
    step_bump(&s, f.root(), &plan).unwrap();
    // the remote refuses the push of main, which is what a ruleset does
    f.git(&["config", "remote.origin.pushurl", "/nonexistent/nowhere.git"]);
    let err = step_merge_and_tag(&s, f.root(), &plan).unwrap_err();
    assert!(matches!(err, ReleaseError::Git(_)), "{err}");
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
    assert!(
        !f.git(&["tag", "--list"]).contains("v0.1.1"),
        "no tag was made"
    );
    let _ = tip;
}
