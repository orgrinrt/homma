//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A release resumed over a `dev` that already carries the bump: the forge
//! release is made from the block the file holds, and a `dev` that moved while
//! the pull request was open is taken once the commit under the merge has a
//! green run.

use super::dev_pr::{BRANCH, blocks_on, bumps_on, carried, landed_by_hand};
use super::pr::{bare, origin_has, pr_calls, refuse_pushes_to, routed};
use super::*;

/// The body the route was given for the forge release.
fn release_body(runner: &Routed<'_>) -> String {
    runner
        .calls
        .borrow()
        .iter()
        .find(|(c, _)| c.starts_with(&format!("{ROUTE} release ")))
        .and_then(|(_, notes)| notes.clone())
        .expect("the route was asked for the release")
}

#[test]
fn a_dev_that_held_the_block_by_another_subject_releases_with_the_block_it_holds() {
    // Mutation: the release made from the plan's block, which the commits since
    // the last tag rebuild with the hand commit in it and today's date.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["main"]);
    let held = carried(&f, true, true);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&held)), false)
        .unwrap()
        .unwrap();
    assert_eq!(
        release_body(&runner),
        "## 0.1.1 (2026-08-31)\n\n### feat\n\n- feat: the thing"
    );
}

#[test]
fn a_dev_another_pull_request_landed_the_bump_on_releases_with_its_block() {
    // Mutation: the release made from a block rebuilt over the commits after
    // the pull request's, and dated by this run.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    landed_by_hand(&f);
    let runner = Routed::new(&f);
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false)
        .unwrap()
        .unwrap();
    assert_eq!(
        release_body(&runner),
        "## 0.1.1 (2026-09-01)\n\n### feat\n\n- feat: the thing"
    );
}

#[test]
fn a_dev_that_moved_under_the_pull_request_is_taken_once_the_commit_under_the_merge_is_green() {
    // Mutation: the merge never taken, so a `dev` an interloper moved can
    // never be released; or taken with no green run on the commit under it.
    let f = Fixture::new();
    refuse_pushes_to(&f, &["dev", "main"]);
    let tip = f.git(&["rev-parse", "dev"]);
    let runner = Routed::new(&f);
    *runner.interloper.borrow_mut() = true;
    let p = published();
    let writes = routed(&runner);
    let s = setup(&runner, &writes, &p, "dev");
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&tip)), false).unwrap_err();
    assert!(matches!(&err, ReleaseError::NotLanded { .. }), "{err}");
    let merge = bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap();
    let moved = f.git(&["rev-parse", &format!("{merge}^1")]);
    assert_ne!(moved, tip, "the interloper moved dev");
    *runner.interloper.borrow_mut() = false;
    let done = release(&s, f.root(), Level::Patch, Some(&green_run(&moved)), false)
        .unwrap()
        .unwrap();
    assert_eq!(done.bump_sha, merge);
    assert_eq!(f.git(&["rev-parse", "dev"]), merge);
    assert_eq!(
        pr_calls(&runner).len(),
        2,
        "dev's once and the release's: {:?}",
        pr_calls(&runner)
    );
    assert_eq!(bumps_on(&f, "origin/dev"), 1);
    assert_eq!(blocks_on(&f, "dev"), 1);
    let main = f.git(&["rev-parse", "origin/main"]);
    assert_eq!(f.git(&["rev-parse", &format!("{main}^2")]), merge);
    assert_eq!(git::tag_target(f.root(), "v0.1.1").unwrap(), main);
    let body = release_body(&runner);
    assert!(
        f.git(&["show", "dev:CHANGELOG.md"]).contains(&body),
        "the release body is the file's block: {body}"
    );
    assert!(!origin_has(&f, BRANCH), "the branch is dropped");
}
