//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A release resumed from what the forge holds: a bump the trunk already
//! holds, a release pull request the route merged and then reported failed, a
//! release pull request whose result is not what the release asked for, and
//! what is never taken for a stopped bump.

use super::pr::{bare, pr_calls, refuse_pushes_to, routed, subjects_on};
use super::*;

/// The number of `chore: release` commits on `rev`.
fn bumps_on(f: &Fixture, rev: &str) -> usize {
    subjects_on(f, rev)
        .into_iter()
        .filter(|s| s.starts_with("chore: release"))
        .count()
}

#[test]
fn a_bump_the_trunk_already_holds_is_taken_without_a_second_push_or_bump() {
    // Mutation: the bump the remote's trunk holds as its tip, the run stopped
    // before the tag, read as nothing pending, so the plan bumps a second time.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.pr_said_into.borrow_mut() = Some(("main", "forge-route: pr: gh: HTTP 502"));
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::Route(_)), "{err}");
    let bump = f.git(&["rev-parse", "dev"]);
    assert_eq!(
        f.git(&["rev-parse", "origin/dev"]),
        bump,
        "the push of dev was made"
    );
    *runner.pr_said_into.borrow_mut() = None;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, bump, "the commit the first run made");
    assert_eq!(bumps_on(&f, "origin/dev"), 1, "one bump on the forge's dev");
    assert_eq!(f.git(&["rev-parse", "dev^"]), tip);
    let forge_main = bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap();
    assert_eq!(f.git(&["rev-parse", &format!("{forge_main}^2")]), bump);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), done.tag_sha);
    assert_eq!(
        pr_calls(&runner).len(),
        2,
        "the failed one and the one that landed, none for dev: {:?}",
        pr_calls(&runner)
    );
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
fn a_release_merge_landing_on_another_says_what_the_forge_holds_and_the_rerun_refuses_it() {
    // Mutation: `NotLanded` with a tag made anyway, or a rerun that takes the
    // forge's merge by its second parent alone and tags a tree the gate never
    // measured, or that plans and bumps again over it.
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
    *runner.interloper.borrow_mut() = false;
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::NotLanded { at, .. } if *at == forge_main),
        "{err}"
    );
    assert_eq!(pr_calls(&runner).len(), 1, "no second pull request");
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "nothing tagged"
    );
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
    assert_eq!(
        bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap(),
        forge_main
    );
}

#[test]
fn a_release_merge_the_forge_resolved_to_another_tree_is_never_tagged() {
    // Mutation: step 5 taking a merge by its parents alone, or a rerun taking
    // the same merge because its second parent is the bump.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let main_before = f.git(&["rev-parse", "main"]);
    let runner = Routed::new(&f);
    *runner.bad_merge.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    let forge_main = bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap();
    assert_eq!(
        f.git(&["rev-parse", &format!("{forge_main}^1")]),
        main_before
    );
    assert!(
        matches!(&err, ReleaseError::NotLanded { at, .. } if *at == forge_main),
        "{err}"
    );
    assert!(git::tag_target(f.root(), "v0.1.1").is_err());
    *runner.bad_merge.borrow_mut() = false;
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::NotLanded { .. }), "{err}");
    assert!(git::tag_target(f.root(), "v0.1.1").is_err());
    assert_eq!(pr_calls(&runner).len(), 1);
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
fn a_squash_of_the_clone_s_bump_on_the_remote_is_no_landing_of_it() {
    // Mutation: a remote trunk holding a commit with the clone's bump's tree
    // on its parent taken for the bump landed: a squash is no landing here,
    // so the clone's bump and the remote's commit are two commits, not one.
    let f = Fixture::new();
    commit_on_dev(&f, "chore: release 0.1.1");
    let squash =
        // a body of its own, so the squash is never the clone's commit under
        // the same author and second
        f.git(&[
            "commit-tree",
            "HEAD^{tree}",
            "-p",
            "HEAD^",
            "-m",
            "chore: release 0.1.1",
            "-m",
            "squashed by the forge",
        ]);
    f.git(&["push", "--quiet", "--force", "origin", &format!("{squash}:refs/heads/dev")]);
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
        sha:     "0123456789012345678901234567890123456789".into(),
        version: Version::new(0, 1, 1),
        landed:  landing::Landed::Local,
    };
    let err = landing::put_back(f.root(), Some(&pending)).unwrap_err();
    assert!(
        err.to_string()
            .contains("0123456789012345678901234567890123456789"),
        "{err}"
    );
}

#[test]
fn a_bump_that_cannot_be_put_back_is_reported_with_the_refusal_that_stopped_the_run() {
    // Mutation: the put-back's failure returned in place of the refusal.
    let f = Fixture::new();
    let pending = landing::Pending {
        sha:     "0123456789012345678901234567890123456789".into(),
        version: Version::new(0, 1, 1),
        landed:  landing::Landed::Local,
    };
    let stopped: Result<(), ReleaseError> = Err(ReleaseError::NoPullRequest("main".into()));
    let err = landing::put_back_after(f.root(), Some(&pending), stopped).unwrap_err();
    let said = err.to_string();
    assert!(
        said.contains("0123456789012345678901234567890123456789"),
        "{said}"
    );
    assert!(said.contains("the forge takes no push to main"), "{said}");
    // and where the run had not stopped, the put-back's failure stands alone
    let err = landing::put_back_after(f.root(), Some(&pending), Ok(())).unwrap_err();
    assert!(
        matches!(err, ReleaseError::NotPutBack {
            stopped: None,
            ..
        }),
        "{err}"
    );
}
