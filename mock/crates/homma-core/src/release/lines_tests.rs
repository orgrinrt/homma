//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A member on a line of its own, planned against its own tags beside the
//! repo's line. The fixture is tomlfuse's shape in small: a root crate `t` at
//! `0.1.0`, tagged `v0.1.0`, a member `t-s` sharing the repo's version, and a
//! member `t-p` whose `release.toml` puts it on a line of its own.

use homma_api::{Level, Target, Version};

use super::*;
use crate::release::plan::{self, Aim, PlanError};
use crate::release::sh;

pub(crate) struct Repo {
    pub(crate) work: tempfile::TempDir,
    pub(crate) bare: tempfile::TempDir,
}

impl Repo {
    pub(crate) fn root(&self) -> &Path {
        self.work.path()
    }

    pub(crate) fn git(&self, args: &[&str]) -> String {
        let out = sh::run(self.root(), "git", args).unwrap();
        assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
        out.stdout.trim().to_string()
    }

    pub(crate) fn write(&self, path: &str, text: &str) {
        let p = self.root().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    pub(crate) fn commit(&self, subject: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", subject]);
    }

    /// The member `t-p` at `version`, on its own line where `own`.
    pub(crate) fn own_member(&self, version: &str, own: bool) {
        self.write(
            "t-p/Cargo.toml",
            &format!("[package]\nname = \"t-p\"\nversion = \"{version}\"\n"),
        );
        self.write("t-p/lib.rs", "// p\n");
        if own {
            self.write("t-p/release.toml", "shared-version = false\n");
        }
    }

    /// `t` and `t-s` at 0.1.0 tagged `v0.1.0` on `main`, `t-p` at
    /// `0.0.1-rc.1` on its own line, and `dev` one commit past the tag that
    /// touches `t-p` alone; everything pushed, the tree on `dev`.
    pub(crate) fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        let bare = tempfile::tempdir().unwrap();
        assert!(
            sh::run(bare.path(), "git", &["init", "--quiet", "--bare"])
                .unwrap()
                .ok()
        );
        let r = Repo {
            work,
            bare,
        };
        r.git(&["init", "--quiet", "-b", "main"]);
        r.git(&["config", "user.email", "t@t"]);
        r.git(&["config", "user.name", "t"]);
        r.git(&["config", "tag.gpgsign", "false"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r.git(&["remote", "add", "origin", r.bare.path().to_str().unwrap()]);
        r.write(
            "Cargo.toml",
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n\n[workspace]\nmembers = [\"t-s\", \"t-p\"]\n",
        );
        r.write(
            "t-s/Cargo.toml",
            "[package]\nname = \"t-s\"\nversion = \"0.1.0\"\n",
        );
        r.write("lib.rs", "// t\n");
        r.own_member("0.0.1-rc.1", true);
        r.commit("feat: first");
        r.git(&["tag", "-a", "v0.1.0", "-m", "v0.1.0"]);
        r.git(&["switch", "--quiet", "-c", "dev"]);
        r.write("t-p/lib.rs", "// p, read\n");
        r.commit("feat: read a table");
        r.git(&["push", "--quiet", "origin", "main", "dev", "refs/tags/v0.1.0"]);
        r
    }

    pub(crate) fn plan(&self, aim: impl Into<Aim>) -> Result<plan::Plan, PlanError> {
        plan::plan(
            self.root(),
            "t",
            homma_api::Markers::defaults(),
            "dev",
            aim,
            "2026-11-28",
        )
    }
}

fn v(s: &str) -> Version {
    s.parse().unwrap()
}

#[test]
fn a_member_is_on_its_own_line_only_where_its_release_toml_says_shared_version_false() {
    // Mutation: every member read as its own line, or the key's value ignored.
    let r = Repo::new();
    let names: Vec<String> = own(r.root()).into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["t-p"]);
    r.write("t-p/release.toml", "shared-version = true\n");
    assert!(own(r.root()).is_empty());
    std::fs::remove_file(r.root().join("t-p/release.toml")).unwrap();
    assert!(own(r.root()).is_empty());
}

#[test]
fn an_own_tag_names_its_crate_and_version_and_no_other_crates() {
    // Mutation: the crate's name not required before `-v`, so `t-p` reads `t-p-x-v1.0.0`.
    assert_eq!(tag("t-p", &v("0.0.1-rc.1")), "t-p-v0.0.1-rc.1");
    assert_eq!(tag_version("t-p", "t-p-v0.0.1-rc.1"), Some(v("0.0.1-rc.1")));
    assert_eq!(tag_version("t-p", "t-p-x-v1.0.0"), None);
    assert_eq!(tag_version("t-p", "v0.1.0"), None);
    assert!(is_own_tag(&["t-p".into()], "t-p-v0.0.1"));
    assert!(!is_own_tag(&["t-p".into()], "v0.0.1"));
    // the repo's tags read none of them
    assert_eq!(crate::release::check::tag_version("t-p-v0.0.1"), None);
}

#[test]
fn with_no_target_and_nothing_on_the_repos_line_the_own_line_releases_alone() {
    // Mutation: the repo's line made to move, or the own line's commits read from the repo's tag.
    let r = Repo::new();
    let p = r.plan(Aim::OwnLines).unwrap();
    assert!(!p.repo_moves);
    assert_eq!(p.next, v("0.1.0"));
    assert_eq!(p.name(), "t-p 0.0.1-rc.1");
    assert_eq!(p.tags(), ["t-p-v0.0.1-rc.1"]);
    assert_eq!(p.lines.len(), 1);
    let l = &p.lines[0];
    assert_eq!(l.last_tag, None);
    assert_eq!(l.to, v("0.0.1-rc.1"));
    // its first release carries every commit that touched its directory
    let subjects: Vec<&str> = l.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["feat: read a table", "feat: first"]);
    assert!(l.changelog.contains("0.0.1-rc.1"), "{}", l.changelog);
    let publishes: Vec<(String, Version)> = p
        .publishes
        .iter()
        .map(|(_, n, v)| (n.clone(), v.clone()))
        .collect();
    assert_eq!(publishes, [("t-p".to_string(), v("0.0.1-rc.1"))]);
}

#[test]
fn with_no_target_a_commit_on_the_repos_line_refuses_with_the_count() {
    // Mutation: the own line released while `main` would carry the repo's commit unreleased.
    let r = Repo::new();
    r.write("lib.rs", "// t, changed\n");
    r.commit("fix: a thing");
    let err = r.plan(Aim::OwnLines).unwrap_err();
    assert!(matches!(err, PlanError::RepoUnreleased(1)), "{err}");
    assert!(err.to_string().contains("name a `--level`"), "{err}");
}

#[test]
fn a_level_moves_the_repos_line_and_the_own_line_rides_it_each_with_its_own_commits() {
    // Mutation: the own line's commit listed in the repo's block, or its crate published at the repo's version.
    let r = Repo::new();
    r.write("lib.rs", "// t, changed\n");
    r.commit("fix: a thing");
    let p = r.plan(Level::Patch).unwrap();
    assert!(p.repo_moves);
    assert_eq!(p.next, v("0.1.1"));
    assert_eq!(p.name(), "0.1.1");
    assert_eq!(p.tags(), ["v0.1.1", "t-p-v0.0.1-rc.1"]);
    let repo: Vec<&str> = p.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(repo, ["fix: a thing"]);
    assert!(!p.changelog.contains("read a table"), "{}", p.changelog);
    let mut publishes: Vec<(String, Version)> = p
        .publishes
        .iter()
        .map(|(_, n, v)| (n.clone(), v.clone()))
        .collect();
    publishes.sort();
    assert_eq!(publishes, [
        ("t".to_string(), v("0.1.1")),
        ("t-p".to_string(), v("0.0.1-rc.1")),
        ("t-s".to_string(), v("0.1.1")),
    ]);
}

#[test]
fn a_level_with_nothing_on_the_repos_line_refuses_as_nothing_to_release() {
    // Mutation: a version of the repo made out of no commit of its own.
    let r = Repo::new();
    let err = r.plan(Target::Level(Level::Patch)).unwrap_err();
    assert!(matches!(err, PlanError::Nothing), "{err}");
}

/// The repo after a release of `t-p` at `0.0.1-rc.1`: its tag on `main`.
fn released(r: &Repo) {
    r.git(&["switch", "--quiet", "main"]);
    r.git(&["merge", "--quiet", "--no-ff", "-m", "release: t-p 0.0.1-rc.1", "dev"]);
    r.git(&["tag", "-a", "t-p-v0.0.1-rc.1", "-m", "t-p-v0.0.1-rc.1"]);
    r.git(&["switch", "--quiet", "dev"]);
    r.git(&["merge", "--quiet", "--ff-only", "main"]);
}

#[test]
fn an_own_line_that_changed_and_did_not_move_refuses_by_name() {
    // Mutation: the change passed over as a rider's is, a warning, and `main` left carrying it.
    let r = Repo::new();
    released(&r);
    r.write("t-p/lib.rs", "// p, read again\n");
    r.commit("fix: read a key");
    let err = r.plan(Aim::OwnLines).unwrap_err();
    assert!(
        matches!(&err, PlanError::OwnUnmoved { name, tag } if name == "t-p" && tag == "t-p-v0.0.1-rc.1"),
        "{err}"
    );
    // control: moved by one step, it releases from its own tag
    r.own_member("0.0.1-rc.2", true);
    r.write("t-p/lib.rs", "// p, read again\n");
    r.commit("chore: move `t-p` to 0.0.1-rc.2");
    let p = r.plan(Aim::OwnLines).unwrap();
    let l = &p.lines[0];
    assert_eq!(
        (l.from.clone(), l.to.clone()),
        (Some(v("0.0.1-rc.1")), v("0.0.1-rc.2"))
    );
    let subjects: Vec<&str> = l.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, [
        "chore: move `t-p` to 0.0.1-rc.2",
        "fix: read a key"
    ]);
}

#[test]
fn an_own_line_that_moves_by_more_than_a_step_refuses_with_both_numbers() {
    // Mutation: the one-step test skipped for an own line.
    let r = Repo::new();
    released(&r);
    r.own_member("0.0.3", true);
    r.commit("chore: move `t-p` to 0.0.3");
    let err = r.plan(Aim::OwnLines).unwrap_err();
    assert!(
        matches!(&err, PlanError::NotOneStep { what, .. } if what.contains("t-p")),
        "{err}"
    );
    assert!(
        err.to_string().contains("0.0.3") && err.to_string().contains("0.0.1-rc.1"),
        "{err}"
    );
}

#[test]
fn an_unchanged_own_line_stays_and_with_nothing_else_there_is_nothing_to_release() {
    // Mutation: a released own line released again, or an empty plan returned.
    let r = Repo::new();
    released(&r);
    let err = r.plan(Aim::OwnLines).unwrap_err();
    assert!(matches!(err, PlanError::Nothing), "{err}");
    r.write("lib.rs", "// t, changed\n");
    r.commit("fix: a thing");
    let p = r.plan(Level::Patch).unwrap();
    assert!(p.lines.is_empty());
    assert_eq!(p.tags(), ["v0.1.1"]);
    assert!(
        p.publishes.iter().all(|(_, n, _)| n != "t-p"),
        "{:?}",
        p.publishes
    );
}

#[test]
fn the_check_reads_an_own_line_against_its_own_tags_and_no_own_tag_is_stray() {
    // Mutation: an own line held to the repo's tags, its tag reported stray, or `main` at its merge called unreleased.
    let r = Repo::new();
    released(&r);
    r.git(&["push", "--quiet", "origin", "main", "dev", "refs/tags/t-p-v0.0.1-rc.1"]);
    let mut published = crate::release::check::Published::default();
    published
        .versions
        .insert((Registry::CratesIo, "t".into()), vec![v("0.1.0")]);
    published
        .versions
        .insert((Registry::CratesIo, "t-s".into()), vec![v("0.1.0")]);
    published
        .versions
        .insert((Registry::CratesIo, "t-p".into()), vec![
            v("0.0.1-rc.1"),
            v("0.0.1-rc.0"),
        ]);
    let found = crate::release::check::check(&crate::release::check::Inputs {
        root:      r.root(),
        repo:      "t",
        remote:    "origin",
        trunk:     "dev",
        release:   "main",
        target:    None,
        published: &published,
        markers:   homma_api::Markers::defaults(),
    })
    .unwrap();
    let ids: Vec<(&str, &str)> = found
        .iter()
        .map(|f| (f.id.as_str(), f.message.as_str()))
        .collect();
    assert!(!ids.iter().any(|(i, _)| *i == "tag.stray"), "{ids:?}");
    assert!(!ids.iter().any(|(i, _)| *i == "main.unreleased"), "{ids:?}");
    // the repo's tag says nothing of `t-p`
    assert!(
        !ids.iter()
            .any(|(i, m)| *i == "reg.unpublished" && m.contains("t-p")),
        "{ids:?}"
    );
    // its own: 0.0.1-rc.0 has no own tag
    assert!(
        ids.iter()
            .any(|(i, m)| *i == "reg.orphan" && m.contains("t-p 0.0.1-rc.0")),
        "{ids:?}"
    );
    assert!(
        !ids.iter()
            .any(|(i, m)| *i == "reg.orphan" && m.contains("0.0.1-rc.1")),
        "{ids:?}"
    );
    // and its newest own tag unpublished is a half-done release
    published
        .versions
        .insert((Registry::CratesIo, "t-p".into()), vec![]);
    let found = crate::release::check::check(&crate::release::check::Inputs {
        root:      r.root(),
        repo:      "t",
        remote:    "origin",
        trunk:     "dev",
        release:   "main",
        target:    None,
        published: &published,
        markers:   homma_api::Markers::defaults(),
    })
    .unwrap();
    assert!(
        found
            .iter()
            .any(|f| f.id == "reg.unpublished" && f.message.contains("`t-p-v0.0.1-rc.1`")),
        "{found:?}"
    );
}

#[test]
fn a_primary_on_a_line_of_its_own_refuses_by_name() {
    // Mutation: the primary's own line taken, and the repo's line left with no version.
    let r = Repo::new();
    r.write("Cargo.toml", "[workspace]\nmembers = [\"t-p\", \"t-s\"]\n");
    r.commit("chore: make the root a virtual workspace");
    let err = plan::plan(
        r.root(),
        "t-p",
        homma_api::Markers::defaults(),
        "dev",
        Level::Patch,
        "2026-11-28",
    )
    .unwrap_err();
    assert!(
        matches!(&err, PlanError::OwnPrimary(n) if n == "t-p"),
        "{err}"
    );
}

/// A root `Cargo.lock` naming `t`, which depends on `t-p`, and `t-s`, at the
/// given versions of `t-p` and `t-s`.
fn lock(p: &str, s: &str) -> String {
    format!(
        "version = 4\n\n[[package]]\nname = \"t\"\nversion = \"0.1.0\"\ndependencies = [\n \"t-p\",\n]\n\n[[package]]\nname = \"t-p\"\nversion = \"{p}\"\n\n[[package]]\nname = \"t-s\"\nversion = \"{s}\"\n"
    )
}

#[test]
fn a_lock_change_that_only_follows_an_own_lines_move_is_that_lines_alone() {
    // Mutation: any change to `Cargo.lock` read as the repo's, so a release of own lines alone always refuses.
    let r = Repo::new();
    r.write("Cargo.lock", &lock("0.0.1-rc.1", "0.1.0"));
    r.commit("chore: lock the workspace");
    released(&r);
    // the lock is released on the repo's line too, so only what follows is new
    r.git(&["tag", "-a", "v0.1.1", "-m", "v0.1.1", "main"]);
    r.own_member("0.0.1-rc.2", true);
    r.write("t-p/lib.rs", "// p, moved\n");
    r.write("Cargo.lock", &lock("0.0.1-rc.2", "0.1.0"));
    r.commit("chore: move `t-p` to 0.0.1-rc.2");
    let p = r.plan(Aim::OwnLines).unwrap();
    assert_eq!(p.name(), "t-p 0.0.1-rc.2");
    // control: a lock change to a package on no own line is the repo's
    r.write("Cargo.lock", &lock("0.0.1-rc.2", "0.1.1"));
    r.commit("chore: take `t-s` 0.1.1");
    let err = r.plan(Aim::OwnLines).unwrap_err();
    assert!(matches!(err, PlanError::RepoUnreleased(1)), "{err}");
}

/// The lock of `lock`, with `t`'s dependency on `t-p` carrying its version, as
/// cargo writes one where two versions of a package are locked.
fn versioned_lock(p: &str, s: &str) -> String {
    lock(p, s).replace(" \"t-p\",\n", &format!(" \"t-p {p}\",\n"))
}

#[test]
fn a_dependency_on_an_own_line_carrying_its_version_follows_that_lines_move() {
    // Mutation: the own lines' references kept in the dependency lists, so the
    // entry `t-p 0.0.1-rc.1` turning `t-p 0.0.1-rc.2` reads as the repo's change.
    let r = Repo::new();
    r.write("Cargo.lock", &versioned_lock("0.0.1-rc.1", "0.1.0"));
    r.commit("chore: lock the workspace");
    released(&r);
    r.git(&["tag", "-a", "v0.1.1", "-m", "v0.1.1", "main"]);
    r.own_member("0.0.1-rc.2", true);
    r.write("t-p/lib.rs", "// p, moved\n");
    r.write("Cargo.lock", &versioned_lock("0.0.1-rc.2", "0.1.0"));
    r.commit("chore: move `t-p` to 0.0.1-rc.2");
    assert!(
        std::fs::read_to_string(r.root().join("Cargo.lock"))
            .unwrap()
            .contains("\"t-p 0.0.1-rc.2\""),
        "the fixture writes the versioned entry"
    );
    let p = r.plan(Aim::OwnLines).unwrap();
    assert_eq!(p.name(), "t-p 0.0.1-rc.2");
}

#[test]
fn a_pull_requests_merge_keeps_its_subject_in_the_repos_block_beside_own_lines() {
    // Mutation: the repo's commits read through a pathspec walk, which drops a merge the same as its second parent.
    let r = Repo::new();
    r.git(&["switch", "--quiet", "-c", "feature"]);
    r.write("lib.rs", "// t, from the pull request\n");
    r.commit("fix: a thing");
    r.git(&["switch", "--quiet", "dev"]);
    r.git(&[
        "merge",
        "--quiet",
        "--no-ff",
        "-m",
        "Merge pull request #7 from o/feature",
        "feature",
    ]);
    let p = r.plan(Level::Patch).unwrap();
    let repo: Vec<&str> = p.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(repo, [
        "Merge pull request #7 from o/feature",
        "fix: a thing"
    ]);
    assert!(p.changelog.contains("#7"), "{}", p.changelog);
    // the own line's own commit stays its alone
    let own: Vec<&str> = p.lines[0]
        .commits
        .iter()
        .map(|c| c.subject.as_str())
        .collect();
    assert!(!own.contains(&"fix: a thing"), "{own:?}");
}

/// The check's findings on the repo as it stands, everything it names published.
fn found(r: &Repo) -> Vec<(String, String)> {
    let mut published = crate::release::check::Published::default();
    for (n, v) in [("t", "0.1.0"), ("t-s", "0.1.0"), ("t-p", "0.0.1-rc.1")] {
        published
            .versions
            .insert((Registry::CratesIo, n.into()), vec![v.parse().unwrap()]);
    }
    crate::release::check::check(&crate::release::check::Inputs {
        root:      r.root(),
        repo:      "t",
        remote:    "origin",
        trunk:     "dev",
        release:   "main",
        target:    None,
        published: &published,
        markers:   homma_api::Markers::defaults(),
    })
    .unwrap()
    .into_iter()
    .map(|f| (f.id, f.message))
    .collect()
}

#[test]
fn an_own_tag_is_checked_as_a_release_tag_on_main_annotated_and_pushed() {
    // Mutation: the tag checks read the repo's version tags alone, so an own tag off `main`, lightweight or unpushed goes unsaid.
    let r = Repo::new();
    r.git(&["tag", "t-p-v0.0.1-rc.1", "dev"]);
    let ids = found(&r);
    let about = |id: &str| {
        ids.iter()
            .any(|(i, m)| i == id && m.contains("t-p-v0.0.1-rc.1"))
    };
    assert!(about("tag.reachable"), "{ids:?}");
    assert!(about("tag.annotated"), "{ids:?}");
    assert!(about("tag.pushed"), "{ids:?}");
    // control: released as a release makes it, nothing of the three is said of it
    r.git(&["tag", "-d", "t-p-v0.0.1-rc.1"]);
    released(&r);
    r.git(&["push", "--quiet", "origin", "main", "dev", "refs/tags/t-p-v0.0.1-rc.1"]);
    let ids = found(&r);
    let about = |id: &str| {
        ids.iter()
            .any(|(i, m)| i == id && m.contains("t-p-v0.0.1-rc.1"))
    };
    assert!(
        !about("tag.reachable") && !about("tag.annotated") && !about("tag.pushed"),
        "{ids:?}"
    );
    assert!(!about("tag.sha.agrees") && !about("tag.local"), "{ids:?}");
    // the own tag moved here after it was pushed points elsewhere on the remote
    r.git(&["tag", "-f", "-a", "t-p-v0.0.1-rc.1", "-m", "moved", "main^1"]);
    let ids = found(&r);
    assert!(
        ids.iter()
            .any(|(i, m)| i == "tag.sha.agrees" && m.contains("t-p-v0.0.1-rc.1")),
        "{ids:?}"
    );
    // an own tag on the remote and not here
    r.git(&["tag", "-a", "t-p-v0.0.1-rc.0", "-m", "t-p-v0.0.1-rc.0", "main"]);
    r.git(&["push", "--quiet", "origin", "refs/tags/t-p-v0.0.1-rc.0"]);
    r.git(&["tag", "-d", "t-p-v0.0.1-rc.0"]);
    let ids = found(&r);
    assert!(
        ids.iter()
            .any(|(i, m)| i == "tag.local" && m.contains("t-p-v0.0.1-rc.0")),
        "{ids:?}"
    );
}

#[test]
fn the_repos_tag_beside_an_own_tag_on_one_merge_is_no_duplicate_and_two_of_one_line_are() {
    // Mutation: `tag.dupes` reads every tag on a commit alike, so a release of two lines is named a duplicate.
    let r = Repo::new();
    released(&r);
    r.git(&["tag", "-a", "v0.1.1", "-m", "v0.1.1", "main"]);
    let dupes = |r: &Repo| {
        found(r)
            .into_iter()
            .filter(|(i, _)| i == "tag.dupes")
            .count()
    };
    assert_eq!(dupes(&r), 0, "{:?}", found(&r));
    r.git(&["tag", "-a", "t-p-v0.0.1-rc.2", "-m", "t-p-v0.0.1-rc.2", "main"]);
    assert_eq!(dupes(&r), 1, "{:?}", found(&r));
}
