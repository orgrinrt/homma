//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone reports over repositories built here, never the live workspace.
//! The branches report: calls, conflicts and the git failures a branch can carry.

use super::super::branches::{Call, Merged, Related, branches, classify, merged, read, related};
use super::super::commit;
use super::Fx;

/// A trunk with one file, a clean branch, a conflicting one, an empty one, a
/// merged one and the names that are never candidates.
fn branched() -> Fx {
    let fx = Fx::new();
    fx.commit("shared.txt", "line\n");
    fx.git(&["branch", "main"]);
    fx.git(&["branch", "badges"]);
    fx.git(&["switch", "-qc", "clean"]);
    fx.commit("clean.txt", "a\nb\n");
    fx.git(&["switch", "-q", "dev"]);
    fx.git(&["switch", "-qc", "clash"]);
    fx.commit("shared.txt", "from the branch\n");
    fx.git(&["switch", "-q", "dev"]);
    fx.git(&["switch", "-qc", "hollow"]);
    fx.git(&["commit", "-q", "--allow-empty", "-m", "nothing"]);
    fx.git(&["switch", "-q", "dev"]);
    fx.git(&["switch", "-qc", "merged"]);
    fx.commit("merged.txt", "m\n");
    fx.git(&["switch", "-q", "dev"]);
    fx.git(&["merge", "-q", "--no-ff", "-m", "merge merged", "merged"]);
    fx.commit("shared.txt", "from the trunk\n");
    fx
}

#[test]
fn a_branch_the_trunk_lacks_is_listed_with_its_lines_its_conflicts_and_its_call() {
    let fx = branched();
    let found = branches(fx.root(), "dev", "main").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|b| {
            (
                b.name.as_str(),
                b.commits,
                b.changed,
                b.conflicts.clone(),
                b.call,
            )
        })
        .collect();
    assert_eq!(rows, [
        (
            "clash",
            1,
            1 + 1,
            vec!["shared.txt".to_string()],
            Call::MergeTrunk
        ),
        ("clean", 1, 2, vec![], Call::Land),
        ("hollow", 1, 0, vec![], Call::Empty),
    ]);
    assert_eq!(found[1].tip, commit(fx.root(), "clean").unwrap());
}

#[test]
fn a_name_on_origin_is_read_at_origins_tip_and_not_the_stale_local_one() {
    let origin = Fx::new();
    origin.commit("shared.txt", "line\n");
    origin.git(&["switch", "-qc", "work"]);
    origin.commit("w.txt", "one\n");
    origin.git(&["switch", "-q", "dev"]);
    let clone = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let mut cmd = std::process::Command::new("git");
        crate::testing::without_git_identity_env(&mut cmd);
        let out = cmd.arg("-C").arg(clone.path()).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    let mut cmd = std::process::Command::new("git");
    crate::testing::without_git_identity_env(&mut cmd);
    let url = origin.root().to_str().unwrap().to_string();
    let out = cmd
        .args(["clone", "-q", &url])
        .arg(clone.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    run(&["switch", "-q", "-c", "work", "origin/work"]);
    run(&["switch", "-q", "dev"]);
    // origin's `work` moves on; the clone's local `work` stays where it was
    origin.git(&["switch", "-q", "work"]);
    origin.commit("w.txt", "two\nthree\n");
    run(&["fetch", "-q", "origin"]);
    let found = branches(clone.path(), "dev", "main").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "work");
    assert_eq!(found[0].commits, 2);
    assert_eq!(
        found[0].tip,
        origin.git(&["rev-parse", "work"]),
        "the remote's tip, not the stale local one"
    );
}

#[test]
fn a_branch_with_no_shared_history_is_called_unrelated_and_the_rest_are_still_listed() {
    let fx = branched();
    fx.git(&["switch", "-q", "--orphan", "island"]);
    fx.commit("island.txt", "alone\n");
    fx.git(&["switch", "-q", "dev"]);
    let found = branches(fx.root(), "dev", "main").unwrap();
    let rows: Vec<_> = found.iter().map(|b| (b.name.as_str(), b.call)).collect();
    assert_eq!(rows, [
        ("clash", Call::MergeTrunk),
        ("clean", Call::Land),
        ("hollow", Call::Empty),
        ("island", Call::Unrelated),
    ]);
    let island = found.iter().find(|b| b.name == "island").unwrap();
    assert!(island.conflicts.is_empty());
    assert_eq!(island.changed, 0);
}

#[test]
fn a_branch_that_changes_only_a_binary_file_is_not_empty() {
    let fx = Fx::new();
    fx.commit("shared.txt", "line\n");
    fx.git(&["switch", "-qc", "blob"]);
    std::fs::write(fx.root().join("b.bin"), [0u8, 1, 2, 0, 255]).unwrap();
    fx.git(&["add", "b.bin"]);
    fx.git(&["commit", "-qm", "blob"]);
    fx.git(&["switch", "-q", "dev"]);
    let found = branches(fx.root(), "dev", "main").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].changed, 0);
    assert_eq!(found[0].call, Call::Land);
}

#[test]
fn two_binary_edits_of_one_file_conflict_on_that_file_and_the_report_goes_on() {
    let fx = Fx::new();
    std::fs::write(fx.root().join("b.bin"), [0u8, 1, 2]).unwrap();
    fx.git(&["add", "b.bin"]);
    fx.git(&["commit", "-qm", "seed"]);
    fx.git(&["switch", "-qc", "left"]);
    std::fs::write(fx.root().join("b.bin"), [0u8, 9, 9, 9]).unwrap();
    fx.git(&["commit", "-qam", "left"]);
    fx.git(&["switch", "-q", "dev"]);
    std::fs::write(fx.root().join("b.bin"), [0u8, 7, 7, 7, 7]).unwrap();
    fx.git(&["commit", "-qam", "trunk"]);
    fx.commit("after.txt", "a\n");
    fx.git(&["switch", "-qc", "right"]);
    fx.commit("right.txt", "r\n");
    fx.git(&["switch", "-q", "dev"]);
    let found = branches(fx.root(), "dev", "main").unwrap();
    let left = found.iter().find(|b| b.name == "left").unwrap();
    assert_eq!(left.call, Call::MergeTrunk, "{left:?}");
    assert_eq!(left.conflicts, ["b.bin"]);
    assert_eq!(left.reason, None);
    let right = found.iter().find(|b| b.name == "right").unwrap();
    assert_eq!(right.call, Call::Land);
}

#[test]
fn only_exit_one_of_merge_base_is_no_common_history_and_any_other_failure_is_unreadable() {
    assert_eq!(related(Some(0), "", "abc\n"), Related::Yes);
    assert_eq!(related(Some(1), "", ""), Related::No);
    assert_eq!(
        related(Some(128), "fatal: Not a valid object name zz\n", ""),
        Related::Unknown("fatal: Not a valid object name zz".to_string())
    );
    // killed by a signal: no exit code at all
    assert_eq!(
        related(None, "", ""),
        Related::Unknown("git gave no reason".to_string())
    );
    // git's output stands in where its error stream is empty
    assert_eq!(
        related(Some(2), "  \n", "\nwords on stdout\nmore"),
        Related::Unknown("words on stdout".to_string())
    );
}

#[test]
fn a_branch_whose_tip_git_cannot_read_is_unreadable_with_its_reason() {
    let fx = Fx::new();
    fx.commit("a.txt", "one\n");
    let ghost = "0123456789abcdef0123456789abcdef01234567";
    let b = read(fx.root(), "dev", "ghost".to_string(), ghost.to_string());
    assert_eq!(b.call, Call::Unreadable);
    assert_eq!(b.commits, 0);
    assert_eq!(b.changed, 0);
    assert!(b.conflicts.is_empty());
    let why = b.reason.expect("a reason");
    assert!(why.contains("git") && why.contains(ghost), "{why}");
}

#[test]
fn a_merge_git_refuses_to_make_is_refused_with_its_words_and_not_a_conflict() {
    let fx = Fx::new();
    fx.commit("a.txt", "one\n");
    let ghost = "0123456789abcdef0123456789abcdef01234567";
    match merged(fx.root(), "dev", ghost).unwrap() {
        Merged::Refused(why) => assert!(!why.is_empty()),
        Merged::Clean(t) => panic!("clean {t}"),
        Merged::Conflicted(f) => panic!("conflicted {f:?}"),
    }
}

#[test]
fn merge_tree_exits_are_read_as_clean_conflicted_or_refused() {
    let tree = "0123456789abcdef0123456789abcdef01234567";
    let tree64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert_eq!(
        classify(Some(0), &format!("{tree}\n"), ""),
        Merged::Clean(tree.to_string())
    );
    assert_eq!(
        classify(
            Some(1),
            &format!("{tree}\na.txt\nb.txt\n\nAuto-merging a.txt\n"),
            ""
        ),
        Merged::Conflicted(vec!["a.txt".to_string(), "b.txt".to_string()])
    );
    // git's way of refusing a merge it cannot make: exit 1, no tree first
    assert_eq!(
        classify(
            Some(1),
            "",
            "merge-tree: 0123 - not something we can merge\n"
        ),
        Merged::Refused("merge-tree: 0123 - not something we can merge".to_string())
    );
    assert_eq!(
        classify(Some(1), "not a tree line\nmore\n", ""),
        Merged::Refused("not a tree line".to_string())
    );
    // a tree of the wrong length or not hex is not a tree
    assert!(matches!(
        classify(Some(1), "abc\nf\n", ""),
        Merged::Refused(_)
    ));
    assert!(matches!(
        classify(Some(1), &format!("{}z\nf\n", &tree[.. 39]), ""),
        Merged::Refused(_)
    ));
    // sha-256 repositories write 64 hex digits
    assert_eq!(
        classify(Some(1), &format!("{tree64}\nf.txt\n"), ""),
        Merged::Conflicted(vec!["f.txt".to_string()])
    );
    assert_eq!(
        classify(Some(128), "", "fatal: bad object\n"),
        Merged::Refused("fatal: bad object".to_string())
    );
    assert_eq!(
        classify(None, "", ""),
        Merged::Refused("git gave no reason".to_string())
    );
    // an exit other than 0 or 1 is a refusal even with a tree on stdout
    assert!(matches!(
        classify(Some(2), &format!("{tree}\n"), ""),
        Merged::Refused(_)
    ));
}
