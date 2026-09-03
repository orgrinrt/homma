//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The git wrappers against real temporary repositories, so every answer is
//! git's own rather than a parsed guess about it.

use super::*;

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    for args in [
        vec!["init", "--quiet", "-b", "main"],
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["config", "tag.gpgsign", "false"],
    ] {
        git(p, &args).unwrap();
    }
    std::fs::write(p.join("a"), "a").unwrap();
    commit_paths(p, &["a"], "feat: first").unwrap();
    d
}

#[test]
fn a_clean_tree_is_clean_and_an_untracked_file_dirties_it() {
    let d = repo();
    assert!(is_clean(d.path()).unwrap());
    std::fs::write(d.path().join("b"), "b").unwrap();
    assert!(!is_clean(d.path()).unwrap());
}

#[test]
fn the_branch_is_read_and_a_detached_head_is_none() {
    let d = repo();
    assert_eq!(current_branch(d.path()).unwrap().as_deref(), Some("main"));
    let h = head(d.path()).unwrap();
    git(d.path(), &["checkout", "--quiet", &h]).unwrap();
    assert_eq!(current_branch(d.path()).unwrap(), None);
}

#[test]
fn an_annotated_tag_is_told_from_a_lightweight_one_and_points_at_its_commit() {
    let d = repo();
    let h = head(d.path()).unwrap();
    tag_annotated(d.path(), "v0.1.0", &h, "v0.1.0").unwrap();
    git(d.path(), &["tag", "light", &h]).unwrap();
    assert!(tag_is_annotated(d.path(), "v0.1.0").unwrap());
    assert!(!tag_is_annotated(d.path(), "light").unwrap());
    assert_eq!(tag_target(d.path(), "v0.1.0").unwrap(), h);
    let mut t = tags(d.path()).unwrap();
    t.sort();
    assert_eq!(t, vec!["light".to_string(), "v0.1.0".to_string()]);
    assert!(matches!(sha(d.path(), "nope"), Err(GitError::Missing(_))));
}

#[test]
fn subjects_come_newest_first_with_the_pr_number_where_a_merge_carries_one() {
    let d = repo();
    let base = head(d.path()).unwrap();
    std::fs::write(d.path().join("b"), "b").unwrap();
    commit_paths(d.path(), &["b"], "fix: second").unwrap();
    std::fs::write(d.path().join("c"), "c").unwrap();
    commit_paths(d.path(), &["c"], "Merge pull request #7 from x/y").unwrap();
    std::fs::write(d.path().join("d"), "d").unwrap();
    commit_paths(d.path(), &["d"], "docs: rewrite readme (#9)").unwrap();
    let s = subjects(d.path(), &base, "HEAD").unwrap();
    let got: Vec<(&str, Option<u64>)> = s.iter().map(|x| (x.subject.as_str(), x.pr)).collect();
    assert_eq!(got, vec![
        ("docs: rewrite readme (#9)", Some(9)),
        ("Merge pull request #7 from x/y", Some(7)),
        ("fix: second", None),
    ]);
    assert_eq!(pr_number("fix: issue #12 in parser"), None);
}

#[test]
fn a_no_ff_merge_makes_a_two_parent_commit_and_a_tag_lands_on_it() {
    let d = repo();
    let p = d.path();
    git(p, &["switch", "--quiet", "-c", "dev"]).unwrap();
    std::fs::write(p.join("b"), "b").unwrap();
    commit_paths(p, &["b"], "feat: on dev").unwrap();
    switch(p, "main").unwrap();
    let merge = merge_no_ff(p, "dev", "release: 0.1.0").unwrap();
    assert_eq!(parent_count(p, &merge).unwrap(), 2);
    assert!(is_ancestor(p, "dev", "main").unwrap());
    assert!(!is_ancestor(p, "main", "dev").unwrap());
    tag_annotated(p, "v0.1.0", &merge, "v0.1.0").unwrap();
    assert_eq!(tag_target(p, "v0.1.0").unwrap(), merge);
}

#[test]
fn first_parent_touching_lists_the_merge_that_brought_a_change_and_not_the_side() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("Cargo.toml"), "v = 1\n").unwrap();
    commit_paths(p, &["Cargo.toml"], "chore: manifest").unwrap();
    let on_main = head(p).unwrap();
    git(p, &["switch", "--quiet", "-c", "dev"]).unwrap();
    std::fs::write(p.join("Cargo.toml"), "v = 2\n").unwrap();
    commit_paths(p, &["Cargo.toml"], "chore: bump").unwrap();
    let on_dev = head(p).unwrap();
    switch(p, "main").unwrap();
    let merge = merge_no_ff(p, "dev", "release").unwrap();
    std::fs::write(p.join("other"), "o").unwrap();
    commit_paths(p, &["other"], "docs: unrelated").unwrap();
    let got = first_parent_touching(p, "main", "Cargo.toml").unwrap();
    assert_eq!(got, vec![merge.clone(), on_main.clone()]);
    assert!(!got.contains(&on_dev), "the side commit is not on the walk");
    assert!(first_parent_touching(p, "main", "nope").unwrap().is_empty());
}

#[test]
fn an_orphan_branch_holds_only_its_files_and_has_no_parent() {
    let d = repo();
    let p = d.path();
    let c = write_orphan_branch(
        p,
        "badges",
        &[("tests.json", "{\"a\":1}"), ("gate.json", "{}")],
        "badges",
    )
    .unwrap();
    assert_eq!(parent_count(p, &c).unwrap(), 0);
    let mut files = files_on(p, "badges").unwrap();
    files.sort();
    assert_eq!(files, vec![
        ("gate.json".to_string(), "{}".to_string()),
        ("tests.json".to_string(), "{\"a\":1}".to_string()),
    ]);
    assert!(is_clean(p).unwrap());
    assert_eq!(current_branch(p).unwrap().as_deref(), Some("main"));
    let again = write_orphan_branch(p, "badges", &[("gate.json", "x")], "badges").unwrap();
    assert_eq!(files_on(p, "badges").unwrap(), vec![(
        "gate.json".to_string(),
        "x".to_string()
    )]);
    assert_eq!(parent_count(p, &again).unwrap(), 0);
}

#[test]
fn commit_paths_leaves_other_staged_files_alone() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("mine"), "m").unwrap();
    std::fs::write(p.join("theirs"), "t").unwrap();
    git(p, &["add", "theirs"]).unwrap();
    commit_paths(p, &["mine"], "chore: mine").unwrap();
    let status = trimmed(p, &["status", "--porcelain"]).unwrap();
    assert_eq!(status, "A  theirs");
}

#[test]
fn modified_and_untracked_are_told_apart_and_tracked_at_reads_a_rev() {
    let d = repo();
    let p = d.path();
    assert!(modified(p).unwrap().is_empty());
    assert!(untracked(p).unwrap().is_empty());
    std::fs::write(p.join("a"), "changed").unwrap();
    std::fs::create_dir(p.join("d")).unwrap();
    std::fs::write(p.join("d/new"), "n").unwrap();
    assert_eq!(modified(p).unwrap(), vec!["a"]);
    assert_eq!(untracked(p).unwrap(), vec!["d/new"]);
    assert_eq!(tracked_at(p, "HEAD").unwrap(), vec!["a"]);
    assert_eq!(show(p, "HEAD", "a").unwrap().as_deref(), Some("a"));
    assert_eq!(show(p, "HEAD", "d/new").unwrap(), None);
}

#[test]
fn a_remote_answers_its_tags_peeled_and_a_branch_is_pushed_only_once_it_is_there() {
    let d = repo();
    let p = d.path();
    let bare = tempfile::tempdir().unwrap();
    git(bare.path(), &["init", "--quiet", "--bare"]).unwrap();
    git(p, &[
        "remote",
        "add",
        "origin",
        bare.path().to_str().unwrap(),
    ])
    .unwrap();
    assert!(!is_pushed(p, "origin", "main").unwrap());
    push(p, "origin", "main", false).unwrap();
    assert!(is_pushed(p, "origin", "main").unwrap());
    let h = head(p).unwrap();
    tag_annotated(p, "v0.1.0", &h, "v0.1.0").unwrap();
    git(p, &["tag", "light", &h]).unwrap();
    assert!(remote_tags(p, "origin").unwrap().is_empty());
    push(p, "origin", "refs/tags/v0.1.0", false).unwrap();
    push(p, "origin", "refs/tags/light", false).unwrap();
    let tags = remote_tags(p, "origin").unwrap();
    assert_eq!(tags, vec![
        ("light".to_string(), h.clone()),
        ("v0.1.0".to_string(), h.clone())
    ]);
    std::fs::write(p.join("b"), "b").unwrap();
    commit_paths(p, &["b"], "feat: b").unwrap();
    assert!(
        !is_pushed(p, "origin", "main").unwrap(),
        "a new commit is unpushed"
    );
    fetch(p, "origin").unwrap();
    assert!(!is_pushed(p, "origin", "main").unwrap());
}
