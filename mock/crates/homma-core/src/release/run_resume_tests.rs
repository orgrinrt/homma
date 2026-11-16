//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A release resumed from what the forge holds: a pull request the route
//! merged and then reported failed, a pull request whose result is not what
//! the release asked for, the branch a squash leaves, and what is never taken
//! for a stopped bump.

use super::pr::{bare, delete_in, pr_calls, refuse_pushes_to, routed, subjects_on};
use super::*;

/// The number of `chore: release` commits on `rev`.
fn bumps_on(f: &Fixture, rev: &str) -> usize {
    subjects_on(f, rev)
        .into_iter()
        .filter(|s| s.starts_with("chore: release"))
        .count()
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
fn the_squashed_bump_s_branch_is_deleted_through_the_route() {
    // Mutation: `release/<repo>-<version>` left on the forge once the squash
    // is checked, or deleted by a git push rather than through the route.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    let pushed = f.git(&["rev-parse", "origin/release/x-0.1.1"]);
    let root = f.root().to_string_lossy().into_owned();
    let asked = format!("{ROUTE} delete-branch --queue {root} release/x-0.1.1 {pushed}");
    assert!(
        runner.calls.borrow().iter().any(|(c, _)| *c == asked),
        "{:?}",
        runner.calls.borrow()
    );
    assert!(!origin_has(&f, "release/x-0.1.1"));
}

#[test]
fn a_squash_the_route_merged_and_reported_failed_is_taken_on_the_rerun() {
    // Mutation: no fetch before the bump is set aside, so the stale tracking
    // ref shows the local bump one ahead and the rerun lands it a second time.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_after.borrow_mut() = Some("forge-route: pr o/x: gh: HTTP 502");
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::Route(_)), "{err}");
    let stopped = f.git(&["rev-parse", "dev"]);
    *runner.pr_after.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(pr_calls(&runner).len(), 1, "{:?}", pr_calls(&runner));
    let forge_dev = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    assert_eq!(done.bump_sha, forge_dev);
    assert_eq!(bumps_on(&f, &forge_dev), 1, "one bump on the forge's dev");
    assert_eq!(f.git(&["rev-parse", &format!("{forge_dev}^")]), tip);
    assert_eq!(f.git(&["rev-parse", "dev"]), forge_dev);
    assert_eq!(f.git(&["rev-parse", "origin/main^2"]), forge_dev);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    let root = f.root().to_string_lossy().into_owned();
    let asked = format!("{ROUTE} delete-branch --queue {root} release/x-0.1.1 {stopped}");
    assert!(
        runner.calls.borrow().iter().any(|(c, _)| *c == asked),
        "the rerun deletes the branch the first run left: {:?}",
        runner.calls.borrow()
    );
    assert!(!origin_has(&f, "release/x-0.1.1"));
}

#[test]
fn a_release_merge_the_route_merged_and_reported_failed_is_tagged_on_the_rerun() {
    // Mutation: a bump on the remote trunk whose release merge the forge
    // already made read as nothing pending, so the plan bumps a second time.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_after.borrow_mut() = Some("forge-route: pr o/x: gh: HTTP 502");
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::Route(_)), "{err}");
    let bump = f.git(&["rev-parse", "dev"]);
    *runner.pr_after.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(pr_calls(&runner).len(), 1, "{:?}", pr_calls(&runner));
    let forge_main = bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap();
    assert_eq!(done.bump_sha, bump);
    assert_eq!(done.tag_sha, forge_main);
    assert_eq!(f.git(&["rev-parse", &format!("{forge_main}^2")]), bump);
    assert_eq!(f.git(&["rev-parse", "main"]), forge_main);
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), forge_main);
    assert_eq!(f.git(&["rev-parse", "dev"]), bump);
}

#[test]
fn a_squash_landing_on_another_merge_says_what_the_forge_holds_and_the_rerun_stops() {
    // Mutation: `NotLanded` naming no next step, or the clone's dev moved off
    // its bump onto a forge commit nobody checked.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.interloper.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let forge_dev = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    assert!(
        matches!(&err, ReleaseError::NotLanded { at, .. } if *at == forge_dev),
        "{err}"
    );
    let said = err.to_string();
    assert!(said.contains(&forge_dev), "{said}");
    assert!(said.contains("tree.pushed"), "{said}");
    assert_eq!(
        f.git(&["rev-parse", "dev^"]),
        tip,
        "the clone keeps its bump"
    );
    *runner.interloper.borrow_mut() = false;
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::Blocked(found) if found.iter().any(|x| x.id == "tree.pushed")),
        "{err}"
    );
    assert_eq!(pr_calls(&runner).len(), 1);
}

#[test]
fn a_release_merge_landing_on_another_says_what_the_forge_holds_and_the_rerun_tags_it() {
    // Mutation: `NotLanded` with a tag made anyway, or a rerun that plans and
    // bumps again over a release merge the forge holds.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.interloper.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let forge_main = bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap();
    assert!(
        matches!(&err, ReleaseError::NotLanded { at, .. } if *at == forge_main),
        "{err}"
    );
    let said = err.to_string();
    assert!(said.contains(&forge_main), "{said}");
    assert!(said.contains("nothing is tagged"), "{said}");
    assert!(git::tag_target(f.root(), "v0.1.1").is_err());
    let bump = f.git(&["rev-parse", "dev"]);
    *runner.interloper.borrow_mut() = false;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(pr_calls(&runner).len(), 1);
    assert_eq!(done.tag_sha, forge_main);
    assert_eq!(done.bump_sha, bump);
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
}

/// `dev` one commit past the remote's, that commit `subject` and empty.
fn commit_on_dev(f: &Fixture, subject: &str) -> String {
    f.git(&["commit", "--quiet", "--allow-empty", "-m", subject]);
    f.git(&["rev-parse", "HEAD"])
}

/// `set_aside` on the clone as it is takes nothing and moves nothing.
fn takes_nothing(f: &Fixture, trunk: &str) {
    let runner = Routed::new(f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, trunk);
    let head = f.git(&["rev-parse", "HEAD"]);
    let set = landing::set_aside(&s, f.root()).unwrap();
    assert!(set.is_none(), "{set:?}");
    assert_eq!(f.git(&["rev-parse", "HEAD"]), head, "nothing moved");
}

#[test]
fn a_dirty_tree_is_no_pending_bump() {
    let f = Fixture::new();
    commit_on_dev(&f, "chore: release 0.1.1");
    std::fs::write(f.root().join("lib.rs"), "// changed").unwrap();
    takes_nothing(&f, "dev");
}

#[test]
fn a_commit_ahead_that_is_no_bump_is_no_pending_bump() {
    let f = Fixture::new();
    commit_on_dev(&f, "feat: more");
    takes_nothing(&f, "dev");
}

#[test]
fn a_release_subject_with_no_version_is_no_pending_bump() {
    // Mutation: the `chore: release ` prefix matched without reading a version.
    let f = Fixture::new();
    commit_on_dev(&f, "chore: release notes for the docs");
    takes_nothing(&f, "dev");
}

#[test]
fn a_bump_two_commits_past_the_remote_is_no_pending_bump() {
    let f = Fixture::new();
    commit_on_dev(&f, "feat: more");
    commit_on_dev(&f, "chore: release 0.1.1");
    takes_nothing(&f, "dev");
}

#[test]
fn a_trunk_the_remote_lacks_is_no_pending_bump() {
    let f = Fixture::new();
    f.git(&["switch", "--quiet", "-c", "solo"]);
    commit_on_dev(&f, "chore: release 0.1.1");
    takes_nothing(&f, "solo");
}

#[test]
fn a_bump_some_tag_contains_is_no_pending_bump() {
    // Mutation: a released bump on the remote trunk taken for one the forge
    // merged unseen, which would replay a release already made.
    let f = Fixture::new();
    commit_on_dev(&f, "chore: release 0.1.1");
    f.git(&["push", "--quiet", "origin", "dev"]);
    f.git(&["tag", "-a", "v0.1.1", "-m", "v0.1.1"]);
    takes_nothing(&f, "dev");
}

#[test]
fn putting_a_bump_back_that_cannot_be_reset_to_says_so() {
    // Mutation: the reset's error dropped, the trunk left set aside unsaid.
    let f = Fixture::new();
    let pending = landing::Pending {
        sha:      "0123456789012345678901234567890123456789".into(),
        version:  Version::new(0, 1, 1),
        landed:   landing::Landed::Local,
        squashed: None,
    };
    let err = landing::put_back(f.root(), Some(&pending)).unwrap_err();
    assert!(
        err.to_string()
            .contains("0123456789012345678901234567890123456789"),
        "{err}"
    );
}

#[test]
fn the_route_s_deletion_takes_a_branch_already_gone_as_deleted() {
    // the fake's own contract, which the deletion tests above lean on
    let f = Fixture::new();
    assert!(delete_in(f.origin(), "release/x-9.9.9", "0").is_ok());
}
