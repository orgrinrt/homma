//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A release resumed over a `dev` that already carries the bump: the forge
//! release is made from the block the file holds, and a `dev` that moved while
//! the pull request was open is taken once the commit under the merge has a
//! green run.

use super::dev_pr::{BRANCH, carried, landed_by_hand};
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
fn a_dev_that_moved_under_the_pull_request_is_never_released_though_the_moved_dev_is_green() {
    // Mutation: the merge taken because its second parent is a bump and its
    // first parent has a green run, which tags the commits the interloper
    // landed that the bump's changelog block, written over the commit before
    // them, leaves out.
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
    let err = release(&s, f.root(), Level::Patch, Some(&green_run(&moved)), false).unwrap_err();
    assert!(
        matches!(&err, ReleaseError::NoGreenRun { .. }),
        "the merge is no pending bump and the bump has no run: {err:?}"
    );
    assert!(
        git::tag_target(f.root(), "v0.1.1").is_err(),
        "nothing tagged"
    );
    assert_eq!(
        pr_calls(&runner).len(),
        1,
        "no second pull request, none for main: {:?}",
        pr_calls(&runner)
    );
    assert_eq!(
        bare(f.origin(), &["rev-parse", "refs/heads/dev"]).unwrap(),
        merge,
        "the forge's dev is left as it is"
    );
    assert_eq!(
        bare(f.origin(), &["rev-parse", "refs/heads/main"]).unwrap(),
        f.git(&["rev-parse", "origin/main"]),
        "main is not moved"
    );
    assert!(
        origin_has(&f, BRANCH),
        "the branch is left for the person who settles it"
    );
}
