//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release against a `dev` the forge takes only through a pull request:
//! the bump is landed by the run itself as a merge commit, waited for and read
//! back, and a run resumed over a `dev` that already carries the bump, by its
//! own pull request or another's, makes no second commit.

use super::pr::{bare, land_in, origin_has, pr_calls, refuse_pushes_to, routed};
use super::*;

const BRANCH: &str = "release/x-0.1.1";
const FAILED: &str = "forge-route: pr: gh: HTTP 502";

/// The route's call that lands the bump `sha` into `dev`.
fn dev_pr(f: &Fixture, sha: &str) -> String {
    let root = f.root().to_string_lossy().into_owned();
    format!("{ROUTE} pr {root} {BRANCH} dev chore: release 0.1.1 merge {sha}")
}

/// The commits on `rev`'s first parents titled as a bump: a landed bump is one.
fn bumps_on(f: &Fixture, rev: &str) -> usize {
    f.git(&["log", "--first-parent", "--format=%s", rev])
        .lines()
        .filter(|s| *s == "chore: release 0.1.1")
        .count()
}

/// How many times `CHANGELOG.md` on `rev` opens a block for 0.1.1.
fn blocks_on(f: &Fixture, rev: &str) -> usize {
    f.git(&["show", &format!("{rev}:CHANGELOG.md")])
        .matches("## 0.1.1 (")
        .count()
}

/// The merge commit the forge made of the bump on `dev`, checked to be one.
fn merge_on_dev(f: &Fixture, tip: &str) -> String {
    let merge = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    f.git(&["fetch", "--quiet", "origin", "dev"]);
    assert_eq!(
        git::parent_count(f.root(), &merge).unwrap(),
        2,
        "a merge commit"
    );
    assert_eq!(f.git(&["rev-parse", &format!("{merge}^1")]), tip);
    let bump = f.git(&["rev-parse", &format!("{merge}^2")]);
    assert_eq!(f.git(&["rev-parse", &format!("{bump}^")]), tip);
    assert_eq!(
        f.git(&["log", "-1", "--format=%s", &bump]),
        "chore: release 0.1.1"
    );
    assert_eq!(
        f.git(&["log", "-1", "--format=%s", &merge]),
        "chore: release 0.1.1"
    );
    merge
}

#[test]
fn a_dev_that_takes_only_pull_requests_gets_the_bump_through_one_merged_as_a_merge_commit() {
    // Mutation: the refused push of `dev` stopping the run as it did, the pull
    // request squashed so the bump is no ancestor of `dev`, the clone's `dev`
    // left on its own commit beside the forge's, or the branch left behind.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let merge = merge_on_dev(&f, &tip);
    let bump = f.git(&["rev-parse", &format!("{merge}^2")]);
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(done.bump_sha, merge, "the bump as the forge's dev holds it");
    assert_eq!(pr_calls(&runner), vec![
        dev_pr(&f, &bump),
        format!("{ROUTE} pr {root} dev main release: 0.1.1 merge {merge}"),
    ]);
    assert_eq!(
        f.git(&["rev-parse", "dev"]),
        merge,
        "the clone's dev is the forge's"
    );
    assert!(!origin_has(&f, BRANCH), "the branch is dropped");
    assert!(
        runner
            .calls
            .borrow()
            .iter()
            .any(|(c, _)| *c == format!("{ROUTE} delete-branch {root} {BRANCH} {bump}")),
        "dropped through the route: {:?}",
        runner.calls.borrow()
    );
    assert_eq!(blocks_on(&f, "dev"), 1);
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), merge);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert_eq!(
        git::current_branch(f.root()).unwrap().as_deref(),
        Some("dev")
    );
    assert!(git::is_clean(f.root()).unwrap());
}

#[test]
fn a_dev_the_client_cannot_land_stays_refused_with_the_bump_on_dev_and_no_branch_beside_it() {
    // Mutation: the client's git push retried or reported as done, or the
    // branch the pull request would have come from left on the forge.
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
        matches!(&err, ReleaseError::NoPullRequest(base) if base == "dev"),
        "{err}"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
    assert_eq!(f.git(&["rev-parse", "dev^"]), tip, "the bump stays on dev");
    assert!(!origin_has(&f, BRANCH), "no branch beside dev");
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
fn a_pull_request_the_route_failed_is_taken_again_and_makes_no_second_bump() {
    // Mutation: the rerun committing and landing a second bump, a second pull
    // request opened, or the commit the first run made dropped from `dev`.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_said_into.borrow_mut() = Some(("dev", FAILED));
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::Route(_)), "{err}");
    let stopped = f.git(&["rev-parse", "dev"]);
    assert_eq!(
        f.git(&["rev-parse", "dev^"]),
        tip,
        "the stop left the bump on dev"
    );
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), tip);
    assert_eq!(
        bare(f.origin(), &["rev-parse", &format!("refs/heads/{BRANCH}")]).unwrap(),
        stopped,
        "the branch the pull request is from holds the bump"
    );
    *runner.pr_said_into.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let merge = merge_on_dev(&f, &tip);
    assert_eq!(
        f.git(&["rev-parse", &format!("{merge}^2")]),
        stopped,
        "the first run's commit"
    );
    assert_eq!(done.bump_sha, merge);
    assert_eq!(pr_calls(&runner).len(), 3, "{:?}", pr_calls(&runner));
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
    assert_eq!(blocks_on(&f, "dev"), 1);
    assert!(!origin_has(&f, BRANCH));
}

#[test]
fn a_pull_request_the_route_merged_and_reported_failed_is_taken_as_landed_on_the_rerun() {
    // Mutation: the merge on `dev` read as nothing pending, so the rerun
    // bumps again and prepends the changelog block a second time over it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_after.borrow_mut() = Some(FAILED);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::Route(_)), "{err}");
    let merge = merge_on_dev(&f, &tip);
    *runner.pr_after.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, merge);
    assert_eq!(
        f.git(&["rev-parse", "origin/dev"]),
        merge,
        "dev is not moved again"
    );
    assert_eq!(f.git(&["rev-parse", "dev"]), merge);
    assert_eq!(
        pr_calls(&runner).len(),
        2,
        "one for dev and the release's: {:?}",
        pr_calls(&runner)
    );
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
    assert_eq!(blocks_on(&f, "dev"), 1, "the block once");
    assert!(!origin_has(&f, BRANCH), "the branch dropped on the rerun");
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), merge);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), main);
}

/// What a person does by hand on a protected `dev`: the bump on a branch of its
/// own, landed into `dev` of the origin by a merge commit, the clone not told.
/// Returns the bump and the merge.
fn landed_by_hand(f: &Fixture) -> (String, String) {
    f.git(&["switch", "--quiet", "-c", BRANCH]);
    std::fs::write(
        f.root().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
    )
    .unwrap();
    std::fs::write(
        f.root().join("CHANGELOG.md"),
        "# Changelog\n\n## 0.1.1 (2026-09-01)\n\n### feat\n\n- feat: the thing\n",
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "--quiet", "-m", "chore: release 0.1.1"]);
    let bump = f.git(&["rev-parse", "HEAD"]);
    f.git(&["push", "--quiet", "origin", BRANCH]);
    f.git(&["switch", "--quiet", "dev"]);
    let root = f.root().to_string_lossy().into_owned();
    land_in(
        f.origin(),
        &["pr", &root, BRANCH, "dev", "chore: release 0.1.1", "merge", &bump],
        false,
    )
    .unwrap();
    (
        bump,
        bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap(),
    )
}

#[test]
fn a_bump_landed_on_dev_by_another_pull_request_is_taken_with_no_commit_and_no_second_block() {
    // Mutation: the coordinator's merge commit read as no bump, so the run
    // plans over a clone one commit behind and bumps and prepends again, as the
    // renki run did.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let (_bump, merge) = landed_by_hand(&f);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, merge);
    assert_eq!(
        f.git(&["rev-parse", "dev"]),
        merge,
        "the clone's dev took it"
    );
    assert_eq!(
        f.git(&["rev-parse", "origin/dev"]),
        merge,
        "nothing was pushed to dev"
    );
    let root = f.root().to_string_lossy().into_owned();
    assert_eq!(pr_calls(&runner), vec![format!(
        "{ROUTE} pr {root} dev main release: 0.1.1 merge {merge}"
    )]);
    assert_eq!(bumps_on(&f, "dev"), 1);
    assert_eq!(blocks_on(&f, "dev"), 1);
    assert!(
        !origin_has(&f, BRANCH),
        "the branch the person left is dropped"
    );
    assert_eq!(
        git::tag_target(f.root(), "v0.1.1").unwrap(),
        f.git(&["rev-parse", "origin/main"])
    );
}

/// A commit on `dev` that moves the manifest to 0.1.1 and, where `block`,
/// opens the changelog for it, under a subject that reads as no bump; pushed.
fn carried(f: &Fixture, manifest: bool, block: bool) -> String {
    if manifest {
        std::fs::write(
            f.root().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
        )
        .unwrap();
    }
    if block {
        std::fs::write(
            f.root().join("CHANGELOG.md"),
            "# Changelog\n\n## 0.1.1 (2026-08-31)\n\n### feat\n\n- feat: the thing\n",
        )
        .unwrap();
    }
    f.git(&["add", "."]);
    f.git(&["commit", "--quiet", "-m", "docs: the release notes by hand"]);
    f.git(&["push", "--quiet", "origin", "dev"]);
    f.git(&["rev-parse", "dev"])
}

#[test]
fn a_dev_carrying_the_version_and_the_block_under_any_subject_gets_no_commit() {
    // Mutation: the block's text compared instead of its heading, so the
    // commits since the tag, which now hold another, make it a new block.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let held = carried(&f, true, true);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&held)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, held);
    assert_eq!(f.git(&["rev-parse", "dev"]), held, "no commit");
    assert_eq!(f.git(&["rev-parse", "origin/dev"]), held, "no push");
    assert_eq!(blocks_on(&f, "dev"), 1);
    let text = f.git(&["show", "dev:CHANGELOG.md"]);
    assert_eq!(text.lines().filter(|l| l.starts_with("## ")).count(), 1);
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), held);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), main);
}

#[test]
fn a_dev_missing_one_half_of_the_bump_gets_a_commit_of_that_half_alone() {
    // Mutation: both halves written whatever dev holds, which is the second
    // changelog block or a version written over itself.
    let f = Fixture::new();
    let held = carried(&f, true, false);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&held)), false)
        .unwrap()
        .unwrap();
    assert_eq!(f.git(&["rev-parse", "dev^"]), held);
    assert_eq!(
        f.git(&["diff", "--name-only", "dev^", "dev"]),
        "CHANGELOG.md"
    );
    assert_eq!(blocks_on(&f, "dev"), 1);
    // the heading in the file at the old version: only the manifest moves
    let g = Fixture::new();
    let held = carried(&g, false, true);
    let runner = Fake(RefCell::new(Vec::new()));
    let w = client(&forge);
    let s = setup(&runner, &w, &p, "dev");
    release(&s, g.root(), Level::Patch, Some(&green_run(&held)), false)
        .unwrap()
        .unwrap();
    assert_eq!(g.git(&["rev-parse", "dev^"]), held);
    assert_eq!(g.git(&["diff", "--name-only", "dev^", "dev"]), "Cargo.toml");
    assert_eq!(blocks_on(&g, "dev"), 1);
}

#[test]
fn a_pull_request_landing_on_a_dev_that_moved_stops_the_run_and_a_rerun_refuses_it_too() {
    // Mutation: the merge taken because its second parent is the bump, so a
    // release is tagged over a `dev` the gate never measured.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.interloper.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let forge_dev = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    assert!(
        matches!(&err, ReleaseError::NotLanded { base, at, .. } if base == "dev" && *at == forge_dev),
        "{err}"
    );
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "nothing tagged"
    );
    *runner.interloper.borrow_mut() = false;
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::NoGreenRun { .. }), "{err}");
    assert_eq!(pr_calls(&runner).len(), 1, "no second pull request");
    assert_eq!(
        bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap(),
        forge_dev,
        "dev is left as the forge has it"
    );
    assert!(git::tag_target(f.root(), "v0.1.1").is_err());
}

#[test]
fn a_pull_request_the_forge_resolved_to_another_tree_is_never_taken() {
    // Mutation: the merge taken by its parents alone, a bump whose changes the
    // forge dropped tagged, or the clone's dev moved onto it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.bad_merge.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let forge_dev = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    assert!(
        matches!(&err, ReleaseError::NotLanded { base, at, .. } if base == "dev" && *at == forge_dev),
        "{err}"
    );
    assert_ne!(
        f.git(&["rev-parse", "dev"]),
        forge_dev,
        "the clone's dev is not moved onto it"
    );
    *runner.bad_merge.borrow_mut() = false;
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert_eq!(pr_calls(&runner).len(), 1, "no second pull request");
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "nothing tagged"
    );
    assert_eq!(
        bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap(),
        forge_dev
    );
}
