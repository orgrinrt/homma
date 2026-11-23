//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release against a forge whose rulesets take `dev` and `main` only
//! through a pull request: the fixture's origin refuses a push to either the
//! way GitHub does. A refused `dev` stops the run, since the trunk takes a push
//! and nothing else; a refused `main` is the release pull request, which the
//! routed fake lands in the origin itself as a merge commit at the head it was
//! given.

use super::*;

/// The words GitHub's ruleset refuses a push with.
const GH013: &str =
    "GH013: Repository rule violations found. Changes must be made through a pull request.";

/// The origin refuses every push to the named branches, as a ruleset does.
pub(super) fn refuse_pushes_to(f: &Fixture, branches: &[&str]) {
    let refs: Vec<String> = branches.iter().map(|b| format!("refs/heads/{b}")).collect();
    std::fs::create_dir_all(f.origin().join("hooks")).unwrap();
    let hook = f.origin().join("hooks/pre-receive");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nwhile read -r old new ref; do\n  case \" {} \" in *\" $ref \"*) echo \"error: {GH013}\" >&2; exit 1 ;; esac\ndone\nexit 0\n",
            refs.join(" ")
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) fn bare(origin: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-c", "user.name=forge", "-c", "user.email=forge@forge"];
    all.extend_from_slice(args);
    let out = sh::run(origin, "git", &all).map_err(|e| e.to_string())?;
    if out.ok() { Ok(out.stdout.trim().to_string()) } else { Err(out.log()) }
}

/// `pr <root> <head> <base> <title> merge <sha>` done in the origin as the
/// forge does it, past the ruleset: refused where the head is not at `sha`,
/// or the method is not a merge, a merge commit of the two, keeping the base's
/// tree where `bad_merge` is set.
pub(super) fn land_in(origin: &Path, args: &[&str], bad_merge: bool) -> Result<(), String> {
    let ["pr", _root, head, base, title, how, sha] = args else {
        return Err(format!(
            "forge-route: a pull request takes six arguments: {args:?}"
        ));
    };
    if *how != "merge" {
        return Err(format!("forge-route: pr: no merge method {how}"));
    }
    let at = bare(origin, &["rev-parse", &format!("refs/heads/{head}")])?;
    if at != *sha {
        return Err(format!("forge-route: pr: {head} is at {at}, not {sha}"));
    }
    let onto = bare(origin, &["rev-parse", &format!("refs/heads/{base}")])?;
    let tree = if bad_merge {
        bare(origin, &["rev-parse", &format!("{onto}^{{tree}}")])?
    } else {
        bare(origin, &["merge-tree", "--write-tree", &onto, sha])?
    };
    let made = bare(origin, &[
        "commit-tree",
        &tree,
        "-p",
        &onto,
        "-p",
        sha,
        "-m",
        title,
    ])?;
    bare(origin, &[
        "update-ref",
        &format!("refs/heads/{base}"),
        &made,
        &onto,
    ])
    .map(|_| ())
}

pub(super) fn routed<'a>(runner: &'a Routed<'a>) -> ByRoute<'a> {
    ByRoute {
        route: Path::new(ROUTE),
        runner,
    }
}

pub(super) fn pr_calls(runner: &Routed<'_>) -> Vec<String> {
    let pre = format!("{ROUTE} pr ");
    runner
        .calls
        .borrow()
        .iter()
        .filter(|(c, _)| c.starts_with(&pre))
        .map(|(c, _)| c.clone())
        .collect()
}

pub(super) fn subjects_on(f: &Fixture, rev: &str) -> Vec<String> {
    f.git(&["log", "--format=%s", rev])
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether the origin holds `branch`.
fn origin_has(f: &Fixture, branch: &str) -> bool {
    bare(f.origin(), &[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("refs/heads/{branch}"),
    ])
    .is_ok()
}

#[test]
fn a_trunk_that_takes_no_push_stops_the_run_and_opens_no_pull_request() {
    // Mutation: the refused push of `dev` landed through a pull request and a
    // squash instead of stopping, a branch pushed beside `dev`, or the bump
    // dropped from the clone's `dev` so the rerun has nothing to push.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Git(_)) && err.to_string().contains("GH013"),
        "git's own words, not a pull request: {err}"
    );
    assert_eq!(pr_calls(&runner), Vec::<String>::new(), "no pull request");
    assert!(!origin_has(&f, "release/x-0.1.1"), "no branch beside dev");
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
    assert_eq!(f.git(&["rev-parse", "dev^"]), tip, "the bump stays on dev");
    assert_eq!(subjects_on(&f, "dev")[0], "chore: release 0.1.1");
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "nothing tagged"
    );
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_trunk_the_forge_takes_gets_the_bump_by_a_push_and_main_the_one_pull_request() {
    // Mutation: the bump landed as a new commit on a new sha, a squash, rather
    // than the commit the run made; or a pull request opened for `dev`.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(pr_calls(&runner), vec![format!(
        "{ROUTE} pr {root} dev main release: 0.1.1 merge {}",
        done.bump_sha
    )]);
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), done.bump_sha);
    assert_eq!(f.git(&["rev-parse", "dev"]), done.bump_sha);
    assert_eq!(f.git(&["rev-parse", "dev^"]), tip);
    assert!(!origin_has(&f, "release/x-0.1.1"));
}

#[test]
fn a_release_line_that_takes_no_push_gets_the_release_pull_request_merged() {
    // Mutation: the refused push of `main` reported as the run's failure, the
    // pull request squashed, or the tag put on the clone's own local merge
    // rather than on the commit the forge made.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let main_before = f.git(&["rev-parse", "main"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(pr_calls(&runner), vec![format!(
        "{ROUTE} pr {root} dev main release: 0.1.1 merge {}",
        done.bump_sha
    )]);
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(done.tag_sha, main);
    assert_eq!(
        f.git(&["rev-parse", "main"]),
        main,
        "the clone's main is the forge's"
    );
    assert_eq!(git::parent_count(f.root(), &main).unwrap(), 2);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^1")]), main_before);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), done.bump_sha);
    assert_eq!(subjects_on(&f, &main)[0], "release: 0.1.1");
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), main);
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_rerun_after_a_stop_at_the_bump_lands_the_same_commit_and_makes_no_second() {
    // Mutation: the rerun refused on its own unpushed bump, gated or planned
    // on it, or bumping and committing again on top of it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(err.to_string().contains("GH013"), "{err}");
    let stopped = f.git(&["rev-parse", "dev"]);
    assert_eq!(
        f.git(&["rev-parse", "dev^"]),
        tip,
        "the stop left the bump on dev"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
    // the forge takes the push of `dev` now, as it does where the ruleset is
    // lifted, and `main` still wants its pull request
    refuse_pushes_to(&f, &["main"]);
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(pr_calls(&runner), vec![format!(
        "{ROUTE} pr {root} dev main release: 0.1.1 merge {}",
        done.bump_sha
    )]);
    assert_eq!(
        done.bump_sha, stopped,
        "the commit the first run made is the one pushed"
    );
    let bumps = subjects_on(&f, "origin/dev")
        .into_iter()
        .filter(|s| s.starts_with("chore: release"))
        .count();
    assert_eq!(bumps, 1, "one bump commit on dev");
    assert_eq!(f.git(&["rev-parse", "origin/dev^"]), tip);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert_eq!(f.git(&["rev-parse", "dev"]), done.bump_sha);
}

#[test]
fn a_rerun_planning_another_version_than_the_stopped_bump_refuses_and_puts_it_back() {
    // Mutation: a stopped `chore: release 0.1.1` taken back under a plan for
    // 0.2.0, or the commit left set aside when the run refuses.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let stopped = f.git(&["rev-parse", "dev"]);
    refuse_pushes_to(&f, &[]);
    let err = release(&s, f.root(), Level::Minor, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::PendingBump { found, planned } if found == "chore: release 0.1.1" && planned == "0.2.0"),
        "{err}"
    );
    assert_eq!(
        f.git(&["rev-parse", "dev"]),
        stopped,
        "the stopped bump is back on dev"
    );
    // and a step before the bump refusing puts it back as well
    let err = release(&s, f.root(), Level::Patch, None::<&GateRun>, false).unwrap_err();
    assert!(matches!(err, ReleaseError::NoGreenRun { .. }), "{err}");
    assert_eq!(f.git(&["rev-parse", "dev"]), stopped);
}

#[test]
fn a_refused_push_of_dev_says_the_same_with_the_client_as_with_the_route() {
    // Mutation: the client's git push retried or reported as done, or the
    // refusal worded as a missing route where `dev` takes no pull request at all.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Git(_)) && err.to_string().contains("GH013"),
        "{err}"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
}

#[test]
fn a_refused_push_of_main_with_no_route_stays_refused_with_its_reason() {
    // Mutation: the client's git push retried or reported as done.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::NoPullRequest(base) if base == "main"),
        "{err}"
    );
}

/// Another merge landed on `base` in the origin, a commit adding a file of its
/// own on its tip, as one landing between the release's push and its pull
/// request does. Made in a clone of the origin and pushed past the ruleset.
pub(super) fn interlope(origin: &Path, base: &str) -> Result<(), String> {
    let at = tempfile::tempdir().map_err(|e| e.to_string())?;
    let o = origin.to_string_lossy();
    let git = |args: &[&str]| -> Result<String, String> {
        let mut all = vec!["-c", "user.name=other", "-c", "user.email=other@other"];
        all.extend_from_slice(args);
        let out = sh::run(at.path(), "git", &all).map_err(|e| e.to_string())?;
        if out.ok() { Ok(out.stdout.trim().to_string()) } else { Err(out.log()) }
    };
    git(&["clone", "--quiet", "--branch", base, &o, "."])?;
    std::fs::write(at.path().join("other.txt"), "landed between").map_err(|e| e.to_string())?;
    git(&["add", "other.txt"])?;
    git(&["commit", "--quiet", "-m", "fix: something else"])?;
    git(&["push", "--quiet", "origin", "HEAD:refs/heads/between"])?;
    let onto = bare(origin, &["rev-parse", &format!("refs/heads/{base}")])?;
    let made = bare(origin, &["rev-parse", "refs/heads/between"])?;
    bare(origin, &[
        "update-ref",
        &format!("refs/heads/{base}"),
        &made,
        &onto,
    ])?;
    bare(origin, &["update-ref", "-d", "refs/heads/between"]).map(|_| ())
}

#[test]
fn a_run_recorded_on_the_tip_is_found_beside_a_leftover_local_bump() {
    // Mutation: the run asked for by the local trunk's sha before the bump
    // is set aside, so a leftover bump over the tip the gate measured finds
    // no run on its own sha and the release refuses at step 2.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let stopped = f.git(&["rev-parse", "dev"]);
    assert_ne!(stopped, tip, "the stop left a bump over the tip");
    refuse_pushes_to(&f, &["main"]);
    let asked = std::cell::RefCell::new(Vec::new());
    let done = release(
        &s,
        f.root(),
        Level::Patch,
        ByLookup(|sha: &str| {
            asked.borrow_mut().push(sha.to_string());
            Ok((sha == tip).then(|| green_run(&tip)))
        }),
        false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        *asked.borrow(),
        vec![tip.clone()],
        "asked for the tip, once"
    );
    assert_eq!(f.git(&["rev-parse", "dev"]), done.bump_sha);
}
