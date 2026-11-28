//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release of a member on a line of its own: alone, where the repo's line
//! has nothing, and riding a release of the repo's line, against the shape
//! `lines_tests.rs` builds.

use super::*;
use crate::release::lines::tests::Repo;
use crate::release::plan::Aim;

/// What a run did: its outcome, the commands the runner was handed, and the
/// forge releases made, each its tag and body.
type Ran = (
    Result<Released, ReleaseError>,
    Vec<String>,
    Vec<(String, String)>,
);

fn run(r: &Repo, aim: impl Into<Aim>) -> Ran {
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    let tip = r.git(&["rev-parse", "dev"]);
    let out = release(
        &setup(&runner, &client(&forge), &p, "dev"),
        r.root(),
        aim,
        Some(&green_run(&tip)),
        false,
    )
    .map(|o| o.expect("not a dry run"));
    let calls = runner.0.borrow().clone();
    let releases = forge.releases.borrow().clone();
    (out, calls, releases)
}

#[test]
fn an_own_line_alone_bumps_merges_and_tags_by_its_own_name() {
    // Mutation: the repo's tag made too, the block written to the root's changelog, or the merge named by the repo's version.
    let r = Repo::new();
    let (out, calls, releases) = run(&r, Aim::OwnLines);
    let done = out.unwrap();
    assert_eq!(
        r.git(&["log", "-1", "--format=%s", "dev"]),
        "chore: release t-p 0.0.1-rc.1"
    );
    let block = std::fs::read_to_string(r.root().join("t-p/CHANGELOG.md")).unwrap();
    assert!(
        block.contains("0.0.1-rc.1") && block.contains("read a table"),
        "{block}"
    );
    assert!(!r.root().join("CHANGELOG.md").exists());
    assert_eq!(
        r.git(&["log", "-1", "--format=%s", "main"]),
        "release: t-p 0.0.1-rc.1"
    );
    let main = r.git(&["rev-parse", "main"]);
    assert_eq!(done.tag_sha, main);
    assert_eq!(r.git(&["rev-parse", "t-p-v0.0.1-rc.1^{commit}"]), main);
    assert_eq!(r.git(&["cat-file", "-t", "t-p-v0.0.1-rc.1"]), "tag");
    assert!(
        r.git(&["ls-remote", "--tags", "origin"])
            .contains("refs/tags/t-p-v0.0.1-rc.1")
    );
    assert_eq!(r.git(&["tag", "--list", "v*"]), "v0.1.0");
    assert_eq!(releases.len(), 1, "{releases:?}");
    assert_eq!(releases[0].0, "t-p-v0.0.1-rc.1");
    assert!(releases[0].1.contains("read a table"), "{}", releases[0].1);
    let publishes: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("cargo publish"))
        .collect();
    assert_eq!(publishes, ["cargo publish -p t-p --locked"]);
}

#[test]
fn a_level_releases_the_repos_line_and_the_own_line_on_one_merge_with_both_tags() {
    // Mutation: one of the tags left off the merge, one forge release, or the own line's block in the root's changelog.
    let r = Repo::new();
    r.write("lib.rs", "// t, changed\n");
    r.commit("fix: a thing");
    r.git(&["push", "--quiet", "origin", "dev"]);
    let (out, calls, releases) = run(&r, homma_api::Level::Patch);
    out.unwrap();
    assert_eq!(
        r.git(&["log", "-1", "--format=%s", "dev"]),
        "chore: release 0.1.1"
    );
    let main = r.git(&["rev-parse", "main"]);
    assert_eq!(r.git(&["rev-parse", "v0.1.1^{commit}"]), main);
    assert_eq!(r.git(&["rev-parse", "t-p-v0.0.1-rc.1^{commit}"]), main);
    let root_block = std::fs::read_to_string(r.root().join("CHANGELOG.md")).unwrap();
    assert!(
        root_block.contains("a thing") && !root_block.contains("read a table"),
        "{root_block}"
    );
    let own_block = std::fs::read_to_string(r.root().join("t-p/CHANGELOG.md")).unwrap();
    assert!(
        own_block.contains("read a table") && !own_block.contains("a thing"),
        "{own_block}"
    );
    let tags: Vec<&str> = releases.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(tags, ["v0.1.1", "t-p-v0.0.1-rc.1"]);
    let mut publishes: Vec<&String> = calls
        .iter()
        .filter(|c| c.starts_with("cargo publish"))
        .collect();
    publishes.sort();
    assert_eq!(publishes, [
        "cargo publish -p t --locked",
        "cargo publish -p t-p --locked",
        "cargo publish -p t-s --locked",
    ]);
}

#[test]
fn a_stopped_own_line_release_is_taken_back_by_its_name_and_not_bumped_twice() {
    // Mutation: the bump's subject read as a version only, so a second bump is written over the first.
    let r = Repo::new();
    r.write(
        "t-p/CHANGELOG.md",
        "## 0.0.1-rc.1 - 2026-11-28\n\n- read a table\n",
    );
    r.commit("chore: release t-p 0.0.1-rc.1");
    r.git(&["push", "--quiet", "origin", "dev"]);
    let bump = r.git(&["rev-parse", "dev"]);
    let runner = Fake(RefCell::new(Vec::new()));
    let forge = Recorder::default();
    let p = published();
    // the gate measured the commit before the bump, which the run sets aside
    let measured = r.git(&["rev-parse", "dev^"]);
    let out = release(
        &setup(&runner, &client(&forge), &p, "dev"),
        r.root(),
        Aim::OwnLines,
        Some(&green_run(&measured)),
        false,
    );
    out.unwrap().unwrap();
    assert_eq!(r.git(&["rev-parse", "dev"]), bump);
    assert_eq!(r.git(&["rev-parse", "main^2"]), bump);
}

#[test]
fn the_release_name_reads_back_and_makes_one_branch_name() {
    // Mutation: a name with a space or a comma pushed as a branch git refuses.
    assert_eq!(
        landing::bump_subject("t-p 0.0.1-rc.1, t-q 0.2.0"),
        "chore: release t-p 0.0.1-rc.1, t-q 0.2.0"
    );
    assert_eq!(
        landing::bump_branch("t", "t-p 0.0.1-rc.1, t-q 0.2.0"),
        "release/t-t-p-0.0.1-rc.1-t-q-0.2.0"
    );
    assert_eq!(landing::bump_branch("x", "0.1.1"), "release/x-0.1.1");
    assert_eq!(
        landing::subject_name("chore: release t-p 0.0.1-rc.1").as_deref(),
        Some("t-p 0.0.1-rc.1")
    );
    assert_eq!(
        landing::subject_name("chore: release 0.1.1").as_deref(),
        Some("0.1.1")
    );
    assert_eq!(landing::subject_name("chore: release"), None);
    assert_eq!(
        landing::subject_name("chore: release notes for 0.1.1"),
        None
    );
}
