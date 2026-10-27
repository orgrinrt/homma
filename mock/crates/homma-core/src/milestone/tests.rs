//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone reports over repositories built here, never the live workspace.

use std::path::Path;

use tempfile::TempDir;

use super::branches::{Call, Merged, Related, branches, merged, read, related};
use super::range::{
    PartitionKind,
    ROOT_UNIT,
    is_read,
    kind_of,
    marker_of,
    partition,
    stand_ins,
    unit_of,
};
use super::record::{RECORD_PATH, Source, base, mark, marks};
use super::{MilestoneError, commit};

/// A repository on `dev`, with a committed identity and nothing else.
struct Fx {
    dir: TempDir,
}

impl Fx {
    fn new() -> Self {
        let fx = Fx {
            dir: tempfile::tempdir().unwrap(),
        };
        fx.git(&["init", "-q", "-b", "dev"]);
        fx.git(&["config", "user.email", "t@t"]);
        fx.git(&["config", "user.name", "t"]);
        fx.git(&["config", "commit.gpgsign", "false"]);
        fx.git(&["config", "tag.gpgsign", "false"]);
        fx
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let mut cmd = std::process::Command::new("git");
        crate::testing::without_git_identity_env(&mut cmd);
        let out = cmd.arg("-C").arg(self.root()).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// Write `content` to `path` and commit it; the new commit's sha.
    fn commit(&self, path: &str, content: &str) -> String {
        let full = self.root().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, content).unwrap();
        self.git(&["add", path]);
        self.git(&["commit", "-qm", &format!("edit {path}")]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Commit the record on the current branch, as the caller of `mark` does.
    fn commit_record(&self) {
        self.git(&["add", RECORD_PATH]);
        self.git(&["commit", "-qm", "edit the record"]);
    }
}

#[test]
fn a_mark_is_appended_with_the_commits_own_date_and_base_reads_the_last() {
    let fx = Fx::new();
    let first = fx.commit("a.txt", "one\n");
    let second = fx.commit("a.txt", "two\n");
    let m = mark(fx.root(), "dev", &first, false).unwrap();
    assert_eq!(m.sha, first);
    assert_eq!(m.at, fx.git(&["show", "-s", "--format=%cs", &first]));
    // the record is a file and nothing was committed for it
    assert!(fx.root().join(RECORD_PATH).is_file());
    assert_eq!(fx.git(&["status", "--porcelain"]), "?? .homma/");
    // base reads what the trunk commits, so an uncommitted mark is not yet one
    assert!(matches!(
        base(fx.root(), "dev"),
        Err(MilestoneError::NoMilestone)
    ));
    fx.commit_record();
    assert_eq!(
        base(fx.root(), "dev").unwrap(),
        (first.clone(), Source::Record)
    );
    mark(fx.root(), "dev", &second, false).unwrap();
    let all = marks(fx.root()).unwrap();
    assert_eq!(all.iter().map(|m| m.sha.as_str()).collect::<Vec<_>>(), [
        first.as_str(),
        second.as_str()
    ]);
    assert_eq!(base(fx.root(), "dev").unwrap().0, first);
    fx.commit_record();
    assert_eq!(base(fx.root(), "dev").unwrap().0, second);
}

#[test]
fn base_answers_as_the_trunk_does_from_a_clone_checked_out_on_another_branch() {
    let fx = Fx::new();
    let first = fx.commit("a.txt", "one\n");
    let second = fx.commit("a.txt", "two\n");
    mark(fx.root(), "dev", &first, false).unwrap();
    fx.commit_record();
    fx.git(&["switch", "-qc", "side"]);
    mark(fx.root(), "dev", &second, false).unwrap();
    fx.commit_record();
    // the side branch's record names two marks, the trunk's names one
    assert_eq!(marks(fx.root()).unwrap().len(), 2);
    assert_eq!(base(fx.root(), "dev").unwrap(), (first, Source::Record));
}

#[test]
fn a_mark_that_is_no_commit_off_the_trunk_or_already_marked_is_refused() {
    let fx = Fx::new();
    let on = fx.commit("a.txt", "one\n");
    fx.git(&["switch", "-qc", "side"]);
    let off = fx.commit("b.txt", "side\n");
    fx.git(&["switch", "-q", "dev"]);
    assert!(matches!(
        mark(fx.root(), "dev", "deadbeef", false),
        Err(MilestoneError::NotACommit(_))
    ));
    assert!(matches!(
        mark(fx.root(), "dev", &off, false),
        Err(MilestoneError::NotOnTrunk { .. })
    ));
    mark(fx.root(), "dev", &on, false).unwrap();
    assert!(matches!(
        mark(fx.root(), "dev", &on, false),
        Err(MilestoneError::AlreadyMarked(_))
    ));
    // nothing but the one good mark was written
    assert_eq!(marks(fx.root()).unwrap().len(), 1);
}

#[test]
fn a_tagged_mark_makes_the_tag_and_a_repo_with_only_the_tag_is_answered_by_it() {
    let fx = Fx::new();
    let first = fx.commit("a.txt", "one\n");
    let second = fx.commit("a.txt", "two\n");
    assert!(matches!(
        base(fx.root(), "dev"),
        Err(MilestoneError::NoMilestone)
    ));
    mark(fx.root(), "dev", &first, true).unwrap();
    assert_eq!(fx.git(&["tag", "--list", "milestone/*"]), "milestone/1");
    std::fs::remove_dir_all(fx.root().join(".homma")).unwrap();
    assert_eq!(
        base(fx.root(), "dev").unwrap(),
        (first.clone(), Source::Tag)
    );
    // the record, once there, wins over the tag
    mark(fx.root(), "dev", &second, false).unwrap();
    fx.commit_record();
    assert_eq!(base(fx.root(), "dev").unwrap(), (second, Source::Record));
}

#[test]
fn a_refused_tag_leaves_no_mark_and_a_rerun_is_not_already_marked() {
    let fx = Fx::new();
    let first = fx.commit("a.txt", "one\n");
    // a tag named `milestone` makes git refuse the name `milestone/1`
    fx.git(&["tag", "milestone", &first]);
    assert!(matches!(
        mark(fx.root(), "dev", &first, true),
        Err(MilestoneError::Git { .. })
    ));
    assert!(marks(fx.root()).unwrap().is_empty());
    assert!(!fx.root().join(RECORD_PATH).exists());
    fx.git(&["tag", "-d", "milestone"]);
    mark(fx.root(), "dev", &first, true).unwrap();
    assert_eq!(marks(fx.root()).unwrap().len(), 1);
    assert_eq!(fx.git(&["tag", "--list", "milestone/*"]), "milestone/1");
}

#[test]
fn a_tag_number_already_taken_is_skipped_for_the_next_free_one() {
    let fx = Fx::new();
    let first = fx.commit("a.txt", "one\n");
    let second = fx.commit("a.txt", "two\n");
    fx.git(&["tag", "milestone/1", &first]);
    mark(fx.root(), "dev", &second, true).unwrap();
    assert_eq!(
        fx.git(&["tag", "--list", "milestone/*"]),
        "milestone/1\nmilestone/2"
    );
    assert_eq!(fx.git(&["rev-list", "-n1", "milestone/2"]), second);
}

#[test]
fn a_clone_with_only_origins_trunk_is_answered_by_its_tags_and_marks() {
    let origin = Fx::new();
    let first = origin.commit("a.txt", "one\n");
    origin.git(&["tag", "-a", "milestone/1", &first, "-m", "m"]);
    origin.git(&["switch", "-qc", "elsewhere"]);
    let dir = tempfile::tempdir().unwrap();
    let url = origin.root().to_str().unwrap().to_string();
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--branch", "elsewhere", &url])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    // there is `origin/dev` here and no local `dev`
    assert!(!exists_local(dir.path(), "dev"));
    assert_eq!(
        base(dir.path(), "dev").unwrap(),
        (first.clone(), Source::Tag)
    );
    mark(dir.path(), "dev", &first, false).unwrap();
    assert_eq!(marks(dir.path()).unwrap().len(), 1);
}

fn exists_local(root: &Path, branch: &str) -> bool {
    let mut cmd = std::process::Command::new("git");
    crate::testing::without_git_identity_env(&mut cmd);
    cmd.arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn units_and_kinds_are_read_off_the_path() {
    assert_eq!(unit_of("mock/crates/homma-core/src/x.rs"), "homma-core");
    assert_eq!(unit_of("crates/a/README.md.tmpl"), "a");
    assert_eq!(unit_of("crates/loose.rs"), ROOT_UNIT);
    assert_eq!(unit_of("scripts/run"), ROOT_UNIT);
    assert_eq!(kind_of("a/README.md"), PartitionKind::Document);
    assert_eq!(kind_of("mock/DESIGN.md.tmpl"), PartitionKind::Document);
    assert_eq!(kind_of("docs/guide.txt"), PartitionKind::Document);
    assert_eq!(kind_of("src/docs/x.rs"), PartitionKind::Source);
    assert_eq!(kind_of("notes.md.bak"), PartitionKind::Source);
}

#[test]
fn a_range_splits_by_crate_and_kind_the_most_changed_first() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.commit("crates/a/src/lib.rs", "1\n2\n3\n");
    fx.commit("crates/a/README.md.tmpl", "doc\n");
    fx.commit("crates/b/src/lib.rs", "1\n2\n3\n4\n5\n");
    fx.commit("docs/guide.md", "g\n");
    fx.commit("script.sh", "s\ns\n");
    std::fs::write(fx.root().join("blob.bin"), [0u8, 159, 146, 150, 0]).unwrap();
    fx.git(&["add", "blob.bin"]);
    fx.git(&["commit", "-qm", "blob"]);
    let parts = partition(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = parts
        .iter()
        .map(|p| (p.unit.as_str(), p.kind, p.files, p.changed()))
        .collect();
    assert_eq!(rows, [
        ("b", PartitionKind::Source, 1, 5),
        ("a", PartitionKind::Source, 1, 3),
        (ROOT_UNIT, PartitionKind::Source, 2, 2),
        (ROOT_UNIT, PartitionKind::Document, 1, 1),
        ("a", PartitionKind::Document, 1, 1),
    ]);
}

#[test]
fn a_range_whose_base_the_head_does_not_contain_is_refused() {
    let fx = Fx::new();
    fx.commit("a.txt", "one\n");
    fx.git(&["switch", "-qc", "side"]);
    let off = fx.commit("b.txt", "side\n");
    fx.git(&["switch", "-q", "dev"]);
    fx.commit("a.txt", "two\n");
    assert!(matches!(
        partition(fx.root(), &off, "dev"),
        Err(MilestoneError::NotAncestor { .. })
    ));
    assert!(matches!(
        stand_ins(fx.root(), &off, "dev"),
        Err(MilestoneError::NotAncestor { .. })
    ));
    assert!(matches!(
        partition(fx.root(), "nowhere", "dev"),
        Err(MilestoneError::NotACommit(_))
    ));
}

#[test]
fn a_marker_is_found_by_the_first_in_the_order_and_case_folds_only_where_declared() {
    assert_eq!(marker_of("// FIXME: x, and relaxed"), Some("FIXME"));
    assert_eq!(marker_of("assert!(a); // Relaxed for now"), Some("relaxed"));
    assert_eq!(marker_of("// A Stand-In for the store"), Some("stand-in"));
    assert_eq!(marker_of("A Stand-In for the store"), None);
    assert_eq!(marker_of("x.load(Ordering::Relaxed)"), None);
    assert_eq!(
        marker_of("x.load(Ordering::Relaxed); // relaxed on purpose"),
        Some("relaxed")
    );
    assert_eq!(marker_of(" * a relaxed assertion"), Some("relaxed"));
    // a continuation is `* ` and text; a dereference at the start of a line is code
    assert_eq!(marker_of("*self.relaxed = true;"), None);
    assert_eq!(marker_of("    *stand_in = 1;"), None);
    assert_eq!(marker_of(" *"), None);
    assert_eq!(marker_of("let s = \"relaxed\";"), None);
    assert_eq!(marker_of("// fixme: lowercase is not the marker"), None);
    assert_eq!(marker_of("let todos = 1;"), None);
    assert_eq!(marker_of("#[ignore = \"slow\"]"), Some("#[ignore"));
    assert_eq!(marker_of("nothing to see"), None);
}

#[test]
fn stand_ins_are_the_added_lines_only_at_their_line_in_the_head() {
    let fx = Fx::new();
    // a marker already there, and one the range removes, are not the range's
    fx.commit(
        "old.rs",
        "// FIXME: before the range\nfn keep() {}\n// TODO: removed later\n",
    );
    let start = fx.git(&["rev-parse", "HEAD"]);
    fx.commit("old.rs", "// FIXME: before the range\nfn keep() {}\n");
    fx.commit(
        "new.rs",
        "fn a() {}\n// FIXME: added\nfn b() { todo!(\"later\") }\n",
    );
    fx.commit(
        "tests/t.rs",
        "#[ignore]\nfn t() {}\n// relaxed from equality\n",
    );
    fx.commit("clean.rs", "fn fine() {}\n");
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.marker.as_str()))
        .collect();
    assert_eq!(rows, [
        ("new.rs", 2, "FIXME"),
        ("new.rs", 3, "todo!("),
        ("tests/t.rs", 1, "#[ignore"),
        ("tests/t.rs", 3, "relaxed"),
    ]);
    assert_eq!(found[0].text, "// FIXME: added");
}

#[test]
fn a_binary_file_is_not_read_for_stand_ins() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    std::fs::write(fx.root().join("b.bin"), b"\0FIXME\0TODO\0").unwrap();
    fx.git(&["add", "b.bin"]);
    fx.git(&["commit", "-qm", "bin"]);
    assert!(stand_ins(fx.root(), &start, "dev").unwrap().is_empty());
}

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
fn stand_ins_are_read_in_source_only_and_a_memory_ordering_is_not_one() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.commit(
        "mock/research/notes.md",
        "FIXME in a note\nTODO in a note\n",
    );
    fx.commit("docs/guide.md", "// relaxed in a page\n");
    fx.commit(
        "src/lib.rs",
        "use std::sync::atomic::Ordering;\nfn n() { c.load(Ordering::Relaxed); }\n// FIXME: real\n",
    );
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.marker.as_str()))
        .collect();
    assert_eq!(rows, [("src/lib.rs", 3, "FIXME")]);
}

#[test]
fn a_placeholder_in_data_and_config_is_read_and_one_in_notes_patches_and_histories_is_not() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    let line = "knob = 1 # FIXME(stand-in): tune later\n";
    fx.commit("content/x.toml", line);
    fx.commit("mock/research/n/x.toml", line);
    fx.commit("mock/design_rounds/202610270000/y.toml", line);
    fx.commit("crates/c/mock/research/z.toml", line);
    fx.commit("mock/research/n/fix.patch", "+// FIXME: quoted\n");
    fx.commit("mock/design_rounds/r/.history", "FIXME in a history\n");
    fx.commit("tools/app.js", "// TODO: code is source\n");
    // a data line with no comment opener is no soft marker, whatever it says
    fx.commit(
        "data/rows.json",
        "{\"a\": \"stand-in\", \"b\": \"relaxed\"}\n",
    );
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.marker.as_str()))
        .collect();
    assert_eq!(rows, [
        ("content/x.toml", "FIXME"),
        ("tools/app.js", "TODO")
    ]);
}

#[test]
fn the_places_stand_ins_are_not_read_in_are_named_by_path_alone() {
    for yes in [
        "src/lib.rs",
        "content/x.toml",
        "a.json",
        "x.tsv",
        "t.js",
        "mock/crates/c/src/a.rs",
    ] {
        assert!(is_read(yes), "{yes}");
    }
    for no in [
        "mock/research/a.toml",
        "mock/design_rounds/a/b.rs",
        "k/mock/research/a.toml",
        "a.patch",
        "x/.history",
        "docs/a.toml",
        "README.md",
        "a.md.tmpl",
    ] {
        assert!(!is_read(no), "{no}");
    }
    // a directory that merely ends in the same words is not the place
    assert!(is_read("notmock/research/a.toml"));
}

#[test]
fn a_users_diff_configuration_changes_no_stand_in() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.git(&["config", "diff.noprefix", "true"]);
    fx.git(&["config", "diff.mnemonicPrefix", "true"]);
    // an external driver that cannot run: without `--no-ext-diff` the diff fails
    fx.git(&["config", "diff.external", "no-such-differ-anywhere"]);
    fx.commit("src/lib.rs", "// FIXME: still found\n");
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, "src/lib.rs");
    assert_eq!(found[0].line, 1);
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
