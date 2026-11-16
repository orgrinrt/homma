//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release against a real repository with a bare remote, a runner that
//! records instead of publishing, and a forge that records instead of posting.

use std::cell::RefCell;

use homma_api::{Level, Step, StepOutcome, Version};

use super::*;
use crate::forge::{CommitStatus, CreateRepoSpec, Forge, RepoMetadata};
use crate::release::sh;
use crate::release::writes::{ByClient, ByRoute};

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

    fn commit_known(&self, _: &str, _: &str, _: &str) -> Result<bool, ForgeError> {
        Ok(true)
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

    /// The bare repository the clone's `origin` is.
    fn origin(&self) -> &Path {
        self._bare.path()
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

/// The forge writes of a release made with git and the recording client.
fn client(forge: &Recorder) -> ByClient<'_> {
    ByClient {
        forge,
        owner: "o",
        name: "x",
        remote: "origin",
    }
}

fn setup<'a>(
    runner: &'a dyn Runner,
    forge: &'a dyn ReleaseForge,
    published: &'a Published,
    trunk: &'a str,
) -> Setup<'a> {
    Setup {
        runner,
        forge,
        repo: "x",
        remote: "origin",
        trunk,
        release: "main",
        date: "2026-09-02",
        token: &token,
        served: &served,
        published,
        markers: homma_api::Markers::defaults(),
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
        &setup(&runner, &client(&forge), &p, "dev"),
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
        &setup(&runner, &client(&forge), &p, "dev"),
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
        &setup(&runner, &client(&forge), &p, "dev"),
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
    // the same block, verbatim, is the release body: what went into the
    // changelog on the trunk is what the forge got, every commit line included
    assert!(
        log.contains(releases[0].1.as_str()),
        "the release body is not the changelog's block:\n{}\n---\n{log}",
        releases[0].1
    );
    assert!(releases[0].1.contains("feat: the thing"));
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
    let w = client(&forge);
    let mut s = setup(&runner, &w, &p, "main");
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
    let plan = plan::plan(
        f.root(),
        "x",
        homma_api::Markers::defaults(),
        "dev",
        Level::Patch,
        "d",
    )
    .unwrap();
    assert!(matches!(
        step_bump(&setup(&runner, &client(&forge), &p, "dev"), f.root(), &plan, None),
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
    let plan = plan::plan(
        f.root(),
        "x",
        homma_api::Markers::defaults(),
        "dev",
        Level::Patch,
        "d",
    )
    .unwrap();
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    step_bump(&s, f.root(), &plan, None).unwrap();
    // the remote refuses the push of main, which is what a ruleset does
    let main_before = f.git(&["rev-parse", "main"]);
    f.git(&["config", "remote.origin.pushurl", "/nonexistent/nowhere.git"]);
    let err = step_merge_and_tag(&s, f.root(), &plan, None).unwrap_err();
    assert!(matches!(err, ReleaseError::Git(_)), "{err}");
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
    // and main holds nothing the remote does not: the merge that landed
    // and did not push is gone, so the next plan sees what this one saw
    assert_eq!(
        f.git(&["rev-parse", "main"]),
        main_before,
        "main is back where it was before the merge"
    );
    let again = plan::plan(
        f.root(),
        "x",
        homma_api::Markers::defaults(),
        "dev",
        Level::Patch,
        "d",
    )
    .unwrap();
    assert_eq!(
        again.commits.len(),
        plan.commits.len() + 1,
        "the bump on dev, and nothing carried over from main"
    );
    assert!(
        !f.git(&["tag", "--list"]).contains("v0.1.1"),
        "no tag was made"
    );
    // the trunk holds the bump and nothing past it: the failed merge left
    // no commit behind on dev
    assert_eq!(
        f.git(&["rev-parse", "dev^"]).trim(),
        tip.trim(),
        "dev is one bump past where it started"
    );
}

/// Where the routed tests' forge route is said to be.
const ROUTE: &str = "/ws/route";

/// A runner standing for the workspace's forge route. It records each call
/// with the `FR_NOTES` it was given. Asked for a tag, it makes it in the
/// fixture's origin and nowhere else, with a message of its own, as the
/// console's Worker does, where `makes_tag` is set, and refuses a tag the
/// origin already holds as the route does; with `makes_tag` unset it answers
/// that the tag is made and makes none. Where `said` is set every call of the
/// route fails with it, as a route whose every way failed does.
struct Routed<'a> {
    calls:      RefCell<Vec<(String, Option<String>)>>,
    origin:     &'a Path,
    makes_tag:  bool,
    said:       Option<&'static str>,
    /// Where set, a pull request asked of the route fails with it.
    pr_said:    RefCell<Option<&'static str>>,
    /// Where set, a pull request asked of the route lands and then fails with
    /// it, as a route that merged and lost the answer does.
    pr_after:   RefCell<Option<&'static str>>,
    /// Where set, another commit lands on a pull request's base before the
    /// pull request does.
    interloper: RefCell<bool>,
}

impl<'a> Routed<'a> {
    fn new(f: &'a Fixture) -> Self {
        Routed {
            calls:      RefCell::new(Vec::new()),
            origin:     f.origin(),
            makes_tag:  true,
            said:       None,
            pr_said:    RefCell::new(None),
            pr_after:   RefCell::new(None),
            interloper: RefCell::new(false),
        }
    }
}

/// The message the fake route writes on a tag, which git in the clone never does.
const ROUTE_MADE: &str = "made by the route";

impl Runner for Routed<'_> {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        let notes = env
            .iter()
            .find(|(k, _)| *k == "FR_NOTES")
            .map(|(_, v)| v.to_string());
        self.calls
            .borrow_mut()
            .push((format!("{program} {}", args.join(" ")), notes));
        let answer = |status: i32, stderr: &str| {
            Ok(sh::Output {
                program: program.into(),
                args:    args.iter().map(|a| a.to_string()).collect(),
                status:  Some(status),
                stdout:  String::new(),
                stderr:  stderr.to_string(),
            })
        };
        if program != ROUTE {
            return answer(0, "");
        }
        if let Some(said) = self.said {
            return answer(1, said);
        }
        if args.first() == Some(&"pr") {
            if let Some(said) = *self.pr_said.borrow() {
                return answer(1, said);
            }
            if *self.interloper.borrow() {
                if let Err(said) = pr::interlope(self.origin, args[3]) {
                    return answer(1, &said);
                }
            }
            return match (pr::land_in(self.origin, args), *self.pr_after.borrow()) {
                (Ok(()), None) => answer(0, ""),
                (Ok(()), Some(said)) => answer(1, said),
                (Err(said), _) => answer(1, &said),
            };
        }
        if let ["delete-branch", "--queue", _root, branch, head] = args {
            return match pr::delete_in(self.origin, branch, head) {
                Ok(()) => answer(0, ""),
                Err(said) => answer(1, &said),
            };
        }
        if let (true, ["tag", _root, tag, sha, _message]) = (self.makes_tag, args) {
            let made = sh::run(self.origin, "git", &[
                "-c",
                "user.name=route",
                "-c",
                "user.email=route@route",
                "tag",
                "-a",
                tag,
                sha,
                "-m",
                ROUTE_MADE,
            ])
            .unwrap();
            if !made.ok() {
                return answer(1, &made.log());
            }
        }
        answer(0, "")
    }
}

/// The subject of `tag`'s message in the clone.
fn tag_message(f: &Fixture, tag: &str) -> String {
    f.git(&["for-each-ref", "--format=%(contents:subject)", &format!("refs/tags/{tag}")])
}

#[test]
fn a_configured_forge_route_makes_the_tag_and_the_release_and_the_clone_holds_the_tag() {
    // Mutation: the route read and then the git push of the tag run anyway, or
    // the forge client asked for the release beside it, which in a cloud
    // session stops the release at the proxy's 403 after `main` moved; or the
    // tag the route made left on the remote alone, so the next plan starts
    // from the tag before it.
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let forge = Recorder::default();
    let p = published();
    let writes = ByRoute {
        route:  Path::new(ROUTE),
        runner: &runner,
    };
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    let calls = runner.calls.borrow();
    let tag = format!("{ROUTE} tag {root} v0.1.1 {} v0.1.1", done.tag_sha);
    assert!(calls.iter().any(|(c, _)| *c == tag), "{calls:?}");
    let rel = calls
        .iter()
        .find(|(c, _)| *c == format!("{ROUTE} release {root} v0.1.1"))
        .unwrap_or_else(|| panic!("{calls:?}"));
    assert!(
        rel.1
            .as_deref()
            .is_some_and(|n| n.contains("feat: the thing")),
        "the changelog block is the release's notes: {rel:?}"
    );
    assert!(
        forge.releases.borrow().is_empty(),
        "the forge client made no release"
    );
    assert_eq!(
        git::tag_target(f.root(), "v0.1.1").unwrap(),
        done.tag_sha,
        "the clone holds the tag the route made, on the release's commit"
    );
    assert_eq!(
        tag_message(&f, "v0.1.1"),
        ROUTE_MADE,
        "the clone's tag is the route's, and git made none of its own"
    );
}

#[test]
fn a_second_release_through_the_route_from_the_same_clone_plans_the_next_version() {
    // Mutation: the tag the route made not fetched into the clone, so the
    // second plan starts from v0.1.0, takes the manifest's 0.1.1 as already
    // bumped, and releases 0.1.1 again until the route refuses its tag.
    let f = Fixture::new();
    let runner = Routed::new(&f);
    let p = published();
    let writes = ByRoute {
        route:  Path::new(ROUTE),
        runner: &runner,
    };
    let s = setup(&runner, &writes, &p, "dev");
    let tip = f.git(&["rev-parse", "dev"]);
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    std::fs::write(f.root().join("more.rs"), "// more").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "--quiet", "-m", "fix: the other thing"]);
    f.git(&["push", "--quiet", "origin", "dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    // the first release published 0.1.1, which the check reads off the registry
    let mut p = published();
    p.versions
        .get_mut(&(Registry::CratesIo, "x".into()))
        .unwrap()
        .push(Version::new(0, 1, 1));
    let s = setup(&runner, &writes, &p, "dev");
    let second = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(second.plan.next, Version::new(0, 1, 2));
    assert_eq!(
        second.plan.commits.len(),
        1,
        "the commits since v0.1.1 and nothing before it"
    );
    assert_eq!(git::tag_target(f.root(), "v0.1.2").unwrap(), second.tag_sha);
}

#[test]
fn a_tag_the_remote_holds_on_another_commit_refuses_the_release() {
    // Mutation: the fetched tag taken as made wherever it points, which
    // reports a release whose tag names some other commit as done.
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let elsewhere = f.git(&["rev-parse", "v0.1.0^{commit}"]);
    let made = sh::run(f.origin(), "git", &[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "tag",
        "-a",
        "v0.1.1",
        &elsewhere,
        "-m",
        "v0.1.1",
    ])
    .unwrap();
    assert!(made.ok(), "{}", made.log());
    let mut runner = Routed::new(&f);
    runner.makes_tag = false;
    let p = published();
    let writes = ByRoute {
        route:  Path::new(ROUTE),
        runner: &runner,
    };
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::TagElsewhere { tag, at, .. } if tag == "v0.1.1" && *at == elsewhere),
        "{err}"
    );
    assert!(
        !runner
            .calls
            .borrow()
            .iter()
            .any(|(c, _)| c.starts_with(&format!("{ROUTE} release"))),
        "no release was asked for on a tag that names another commit"
    );
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "the clone keeps no version tag on the wrong commit"
    );
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
}

#[test]
fn a_route_that_answers_done_and_made_no_tag_stops_the_release() {
    // Mutation: the route's answer taken on its word, which reports a release
    // with no tag anywhere as done.
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let mut runner = Routed::new(&f);
    runner.makes_tag = false;
    let p = published();
    let writes = ByRoute {
        route:  Path::new(ROUTE),
        runner: &runner,
    };
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Git(GitError::Failed { command, .. }) if command.contains("refs/tags/v0.1.1")),
        "the fetch of the tag is what refuses: {err}"
    );
    let root = f.root().to_string_lossy().into_owned();
    let calls = runner.calls.borrow();
    assert!(
        calls
            .iter()
            .any(|(c, _)| c.starts_with(&format!("{ROUTE} tag {root} v0.1.1 "))),
        "the route was asked for the tag: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|(c, _)| c.starts_with(&format!("{ROUTE} release"))),
        "no release was asked for on a tag nobody made: {calls:?}"
    );
    assert!(git::tag_target(f.root(), "v0.1.1").is_err());
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
}

#[test]
fn a_forge_route_that_fails_every_way_stops_the_release_with_its_words() {
    // Mutation: the route's refusal ignored, which reports a release with no
    // tag as done.
    let f = Fixture::new();
    let tip = f.git(&["rev-parse", "dev"]);
    let mut runner = Routed::new(&f);
    runner.said = Some("forge-route: tag o/x v0.1.1: git: HTTP 403; forge-write: 401 unauthorized");
    let p = published();
    let writes = ByRoute {
        route:  Path::new(ROUTE),
        runner: &runner,
    };
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Route(said) if said.contains("forge-write: 401 unauthorized")),
        "{err}"
    );
    let calls = runner.calls.borrow();
    assert!(
        !calls.iter().any(|(c, _)| {
            c.starts_with(&format!("{ROUTE} release")) || c.starts_with("cargo publish")
        }),
        "nothing after the tag ran: {calls:?}"
    );
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
}

#[path = "workspace_tests.rs"]
mod workspace;

#[path = "run_pr_tests.rs"]
mod pr;

#[path = "run_resume_tests.rs"]
mod resume;
