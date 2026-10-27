//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone reports over repositories built here, never the live workspace.

use std::path::Path;

use tempfile::TempDir;

use super::MilestoneError;
use super::record::{RECORD_PATH, Source, base, mark, marks};

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

mod branches;
mod range;
