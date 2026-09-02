//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The check against real temporary repositories with a bare remote, and
//! canned registry answers, so every finding is produced by a planted defect.

use super::*;
use crate::release::sh;

struct Fixture {
    work:  tempfile::TempDir,
    _bare: tempfile::TempDir,
}

impl Fixture {
    fn root(&self) -> &Path {
        self.work.path()
    }

    fn git(&self, args: &[&str]) {
        let out = sh::run(self.root(), "git", args).unwrap();
        assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
    }

    /// A crate at `version`, committed on `main`, pushed, with `dev` beside it.
    fn crate_at(version: &str) -> Self {
        let work = tempfile::tempdir().unwrap();
        let bare = tempfile::tempdir().unwrap();
        let out = sh::run(bare.path(), "git", &["init", "--quiet", "--bare"]).unwrap();
        assert!(out.ok());
        let f = Fixture {
            work,
            _bare: bare,
        };
        f.git(&["init", "--quiet", "-b", "main"]);
        f.git(&["config", "user.email", "t@t"]);
        f.git(&["config", "user.name", "t"]);
        f.git(&["config", "tag.gpgsign", "false"]);
        f.git(&["config", "commit.gpgsign", "false"]);
        f.git(&["remote", "add", "origin", f._bare.path().to_str().unwrap()]);
        f.manifest(version);
        f.git(&["add", "Cargo.toml"]);
        f.git(&["commit", "--quiet", "-m", "feat: first"]);
        f.git(&["branch", "dev"]);
        f.git(&["push", "--quiet", "origin", "main", "dev"]);
        f
    }

    fn manifest(&self, version: &str) {
        std::fs::write(
            self.root().join("Cargo.toml"),
            format!("[package]\nname = \"x\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
    }

    fn tag(&self, name: &str) {
        self.git(&["tag", "-a", name, "-m", name]);
        self.git(&["push", "--quiet", "origin", &format!("refs/tags/{name}")]);
    }

    fn commit(&self, subject: &str) {
        let n = unique();
        std::fs::write(self.root().join(format!("f{n}")), "x").unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", subject]);
    }
}

fn published(versions: &[&str]) -> Published {
    let mut p = Published::default();
    p.versions.insert(
        (Registry::CratesIo, "x".into()),
        versions.iter().map(|v| v.parse().unwrap()).collect(),
    );
    p
}

fn ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.id.as_str()).collect()
}

fn run(f: &Fixture, published: &Published, level: Option<Level>) -> Vec<Finding> {
    check(&Inputs {
        root: f.root(),
        remote: "origin",
        trunk: "dev",
        release: "main",
        level,
        published,
    })
    .unwrap()
}

#[test]
fn a_released_repo_in_order_has_no_finding() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_dirty_tree_is_two_errors_and_a_detached_head_a_third() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    f.manifest("0.1.0\n");
    std::fs::write(f.root().join("stray"), "s").unwrap();
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert!(ids(&findings).contains(&"tree.clean"));
    assert!(ids(&findings).contains(&"tree.untracked"));
    assert!(blocked(&findings));
    f.git(&["checkout", "--quiet", "."]);
    std::fs::remove_file(f.root().join("stray")).unwrap();
    f.git(&["checkout", "--quiet", "--detach"]);
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert_eq!(ids(&findings), ["tree.attached"]);
}

#[test]
fn an_unpushed_commit_and_a_workflow_file_block() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    std::fs::create_dir_all(f.root().join(".github/workflows")).unwrap();
    std::fs::write(f.root().join(".github/workflows/ci.yml"), "on: push").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "--quiet", "-m", "chore: workflow"]);
    let findings = run(&f, &published(&["0.1.0"]), None);
    let ids = ids(&findings);
    assert!(ids.contains(&"tree.pushed"), "{ids:?}");
    assert!(ids.contains(&"hist.workflow.tree"), "{ids:?}");
    assert!(ids.contains(&"main.unreleased"), "{ids:?}");
}

#[test]
fn a_tag_off_main_is_unreachable_and_a_lightweight_or_unpushed_one_is_named() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    f.git(&["switch", "--quiet", "dev"]);
    f.manifest("0.1.1");
    f.git(&["commit", "--quiet", "-am", "chore: bump"]);
    f.git(&["tag", "v0.1.1"]);
    f.git(&["push", "--quiet", "origin", "dev"]);
    let findings = run(&f, &published(&["0.1.0", "0.1.1"]), None);
    let ids = ids(&findings);
    assert!(ids.contains(&"tag.reachable"), "{ids:?}");
    assert!(ids.contains(&"tag.annotated"), "{ids:?}");
    assert!(ids.contains(&"tag.pushed"), "{ids:?}");
    assert!(
        findings[0].severity >= findings[findings.len() - 1].severity,
        "blocking first"
    );
}

#[test]
fn a_tag_that_points_elsewhere_on_the_remote_is_fatal_and_a_remote_only_tag_is_a_warning() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    f.git(&["tag", "-d", "v0.1.0"]);
    f.commit("chore: other");
    f.git(&["tag", "-a", "v0.1.0", "-m", "again"]);
    f.git(&["push", "--quiet", "origin", "main"]);
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert_eq!(findings[0].id, "tag.sha.agrees");
    assert_eq!(findings[0].severity, CheckSeverity::Fatal);
    f.git(&["tag", "-d", "v0.1.0"]);
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert!(ids(&findings).contains(&"tag.local"));
}

#[test]
fn main_past_the_newest_tag_blocks_unless_every_commit_is_a_hotpatch() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    f.commit("fix: hotpatch the readme");
    f.git(&["push", "--quiet", "origin", "main"]);
    assert!(run(&f, &published(&["0.1.0"]), None).is_empty());
    f.commit("feat: real work");
    f.git(&["push", "--quiet", "origin", "main"]);
    let findings = run(&f, &published(&["0.1.0"]), None);
    assert_eq!(ids(&findings), ["main.unreleased"]);
    assert!(findings[0].message.contains("2 commit"));
}

#[test]
fn a_version_bump_on_main_carries_a_tag_whether_committed_there_or_merged_in() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    // a bump committed straight onto main, untagged
    f.manifest("0.2.0");
    f.git(&["add", "Cargo.toml"]);
    f.git(&["commit", "--quiet", "-m", "chore: release 0.2.0"]);
    f.git(&["push", "--quiet", "origin", "main"]);
    let findings = run(&f, &published(&["0.1.0", "0.2.0"]), None);
    let bumps: Vec<&Finding> = findings
        .iter()
        .filter(|x| x.id == "tag.bump.tagged")
        .collect();
    assert_eq!(bumps.len(), 1, "{findings:?}");
    assert!(bumps[0].message.contains("0.2.0"));
    assert!(bumps[0].severity.blocks());
    f.tag("v0.2.0");
    assert!(!ids(&run(&f, &published(&["0.1.0", "0.2.0"]), None)).contains(&"tag.bump.tagged"));
    // a bump made on the trunk and merged in: the merge commit is the bump
    // on main's first-parent walk, and it is the merge that wants the tag
    f.git(&["switch", "--quiet", "dev"]);
    f.git(&["merge", "--quiet", "--ff-only", "main"]);
    f.manifest("0.3.0");
    f.git(&["add", "Cargo.toml"]);
    f.git(&["commit", "--quiet", "-m", "chore: release 0.3.0"]);
    f.git(&["switch", "--quiet", "main"]);
    f.git(&["merge", "--quiet", "--no-ff", "-m", "release: 0.3.0", "dev"]);
    f.git(&["push", "--quiet", "origin", "main", "dev"]);
    let findings = run(&f, &published(&["0.1.0", "0.2.0", "0.3.0"]), None);
    let bumps: Vec<&Finding> = findings
        .iter()
        .filter(|x| x.id == "tag.bump.tagged")
        .collect();
    assert_eq!(bumps.len(), 1, "{findings:?}");
    let merge = sh::run(f.root(), "git", &["rev-parse", "main"]).unwrap();
    assert!(bumps[0].message.starts_with(&merge.stdout.trim()[.. 7]));
    // tagging the bump commit on dev rather than the merge does not clear it
    let bump_on_dev = sh::run(f.root(), "git", &["rev-parse", "dev"]).unwrap();
    f.git(&["tag", "-a", "v0.3.0", "-m", "v0.3.0", bump_on_dev.stdout.trim()]);
    f.git(&["push", "--quiet", "origin", "refs/tags/v0.3.0"]);
    assert!(
        ids(&run(&f, &published(&["0.1.0", "0.2.0", "0.3.0"]), None)).contains(&"tag.bump.tagged")
    );
    f.git(&["tag", "-d", "v0.3.0"]);
    f.git(&["push", "--quiet", "--delete", "origin", "refs/tags/v0.3.0"]);
    f.tag("v0.3.0");
    let findings = run(&f, &published(&["0.1.0", "0.2.0", "0.3.0"]), None);
    assert!(!ids(&findings).contains(&"tag.bump.tagged"), "{findings:?}");
}

#[test]
fn the_manifest_at_a_tag_must_equal_the_tag() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.2.0");
    let findings = run(&f, &published(&[]), None);
    assert!(ids(&findings).contains(&"man.version.matches"));
    assert_eq!(findings[0].severity, CheckSeverity::Fatal);
}

#[test]
fn the_working_version_must_be_above_the_published_and_the_smallest_step_at_the_level() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    assert!(
        run(&f, &published(&["0.1.0"]), Some(Level::Patch)).is_empty(),
        "at the published version, the run bumps"
    );
    let findings = run(&f, &published(&["0.1.0", "0.1.1"]), Some(Level::Patch));
    assert_eq!(ids(&findings), ["man.current.forward", "reg.orphan"]);
    f.git(&["switch", "--quiet", "dev"]);
    f.manifest("0.3.0");
    f.git(&["commit", "--quiet", "-am", "chore: jump"]);
    f.git(&["push", "--quiet", "origin", "dev"]);
    let findings = run(&f, &published(&["0.1.0"]), Some(Level::Minor));
    assert_eq!(ids(&findings), ["man.current.smallest"]);
    assert!(
        run(&f, &published(&["0.1.0"]), None).is_empty(),
        "no level, no smallest check"
    );
    f.manifest("0.2.0");
    f.git(&["commit", "--quiet", "-am", "chore: step"]);
    f.git(&["push", "--quiet", "origin", "dev"]);
    assert!(run(&f, &published(&["0.1.0"]), Some(Level::Minor)).is_empty());
}

#[test]
fn the_registry_must_ascend_skip_nothing_and_match_the_tags_both_ways() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    let findings = run(&f, &published(&["0.1.0", "0.3.0", "0.2.0"]), None);
    let ids = ids(&findings);
    assert!(ids.contains(&"order.ascends"), "{ids:?}");
    assert!(
        !ids.contains(&"semver.gaps"),
        "0.1, 0.2, 0.3 skip nothing: {ids:?}"
    );
    assert_eq!(ids.iter().filter(|i| **i == "reg.orphan").count(), 2);
    let findings = run(&f, &published(&["0.1.0", "0.1.2"]), None);
    let ids = self::ids(&findings);
    assert!(ids.contains(&"semver.gaps"), "{ids:?}");
    assert!(!ids.contains(&"order.ascends"), "{ids:?}");
    let findings = run(&f, &published(&[]), None);
    assert_eq!(self::ids(&findings), ["reg.unpublished"]);
    assert!(!blocked(&findings));
}

#[test]
fn jsr_and_npm_disagreeing_is_both_sameset() {
    let f = Fixture::crate_at("0.1.0");
    f.tag("v0.1.0");
    let mut p = published(&["0.1.0"]);
    p.versions
        .insert((Registry::Jsr, "@h/x".into()), vec![Version::new(0, 1, 0)]);
    p.versions.insert((Registry::Npm, "x".into()), vec![]);
    let findings = run(&f, &p, None);
    assert!(ids(&findings).contains(&"both.sameset"));
    assert!(!blocked(&findings));
}

#[test]
fn packages_are_read_off_a_workspace_and_a_private_member_is_skipped() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\", \"tool\"]\n",
    )
    .unwrap();
    for (dir, body) in [
        ("crates/b", "[package]\nname = \"b\"\n"),
        ("crates/a", "[package]\nname = \"a\"\n"),
        ("crates/p", "[package]\nname = \"p\"\npublish = false\n"),
        ("tool", "[package]\nname = \"tool\"\n"),
    ] {
        std::fs::create_dir_all(d.path().join(dir)).unwrap();
        std::fs::write(d.path().join(dir).join("Cargo.toml"), body).unwrap();
    }
    std::fs::write(d.path().join("deno.json"), r#"{"name": "@h/x"}"#).unwrap();
    std::fs::write(d.path().join("package.json"), r#"{"name": "x-npm"}"#).unwrap();
    let p = packages(d.path(), RepoKind::Both);
    assert_eq!(p.crates, ["a", "b", "tool"]);
    assert_eq!(p.jsr.as_deref(), Some("@h/x"));
    assert_eq!(p.npm.as_deref(), Some("x-npm"));
    assert_eq!(p.each().len(), 5);
    let crate_only = packages(d.path(), RepoKind::Crate);
    assert_eq!(crate_only.jsr, None);
}

#[test]
fn the_tag_name_follows_the_repos_convention() {
    let v = Version::new(1, 2, 3);
    assert_eq!(tag_name(&[], &v), "v1.2.3");
    assert_eq!(tag_name(&["v0.1.0".into()], &v), "v1.2.3");
    assert_eq!(tag_name(&["0.1.0".into()], &v), "1.2.3");
    assert_eq!(tag_name(&["0.1.0".into(), "v0.2.0".into()], &v), "v1.2.3");
    assert_eq!(tag_version("v0.1.0"), Some(Version::new(0, 1, 0)));
    assert_eq!(tag_version("0.1.0"), Some(Version::new(0, 1, 0)));
    assert_eq!(tag_version("light"), None);
}

#[test]
fn adjacency_is_one_step_at_any_level_and_zero_to_one_counts() {
    let v = |s: &str| s.parse::<Version>().unwrap();
    assert!(is_adjacent(&v("0.1.0"), &v("0.1.1")));
    assert!(is_adjacent(&v("0.1.0"), &v("0.2.0")));
    assert!(is_adjacent(&v("1.1.4"), &v("2.0.0")));
    assert!(is_adjacent(&v("0.9.9"), &v("1.0.0")));
    assert!(!is_adjacent(&v("0.1.0"), &v("0.1.2")));
    assert!(!is_adjacent(&v("0.1.0"), &v("0.3.0")));
    assert!(!is_adjacent(&v("1.0.0"), &v("3.0.0")));
}
