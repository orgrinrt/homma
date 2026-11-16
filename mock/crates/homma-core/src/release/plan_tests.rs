//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The plan, against temporary repositories.

use homma_api::Level;

use super::*;
use crate::release::sh;

fn repo(version: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| {
        let out = sh::run(d.path(), "git", args).unwrap();
        assert!(out.ok(), "{}", out.log());
    };
    g(&["init", "--quiet", "-b", "main"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    g(&["config", "tag.gpgsign", "false"]);
    g(&["config", "commit.gpgsign", "false"]);
    std::fs::write(
        d.path().join("Cargo.toml"),
        format!("[package]\nname = \"x\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    g(&["add", "."]);
    g(&["commit", "--quiet", "-m", "feat: first"]);
    d
}

fn commit(d: &Path, subject: &str) {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::fs::write(d.join(format!("f{n}")), "x").unwrap();
    for args in [vec!["add", "."], vec!["commit", "--quiet", "-m", subject]] {
        assert!(sh::run(d, "git", &args).unwrap().ok());
    }
}

#[test]
fn a_first_release_carries_the_whole_history_and_bumps_from_the_manifest() {
    let d = repo("0.1.0");
    commit(d.path(), "fix: two");
    let p = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Patch,
        "2026-09-02",
    )
    .unwrap();
    assert_eq!(p.last_tag, None);
    assert_eq!(p.commits.len(), 2);
    assert_eq!(p.commits[0].subject, "fix: two");
    assert_eq!(p.next, Version::new(0, 1, 1));
    assert_eq!(p.tag, "v0.1.1");
    assert!(p.changelog.starts_with("## 0.1.1 (2026-09-02)\n"));
    assert!(p.changelog.contains("### feat"));
    assert!(p.changelog.contains("### fix"));
    assert_eq!(p.publishes, vec![(
        Registry::CratesIo,
        "x".to_string(),
        Version::new(0, 1, 1)
    )]);
    let shown = p.to_string();
    assert!(shown.contains("no release tag yet"));
    assert!(shown.contains("x 0.1.1 to crates-io"));
}

#[test]
fn commits_are_counted_from_the_newest_tag_and_the_tag_follows_the_convention() {
    let d = repo("0.1.0");
    assert!(
        sh::run(d.path(), "git", &["tag", "-a", "0.1.0", "-m", "0.1.0"])
            .unwrap()
            .ok()
    );
    commit(d.path(), "feat: later");
    let p = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Minor,
        "d",
    )
    .unwrap();
    assert_eq!(p.last_tag.as_deref(), Some("0.1.0"));
    assert_eq!(p.commits.len(), 1);
    assert_eq!(p.next, Version::new(0, 2, 0));
    assert_eq!(p.tag, "0.2.0", "bare tags stay bare");
    assert!(p.to_string().contains("since `0.1.0`"));
}

#[test]
fn a_manifest_off_the_level_refuses_and_one_at_the_level_is_the_release() {
    let d = repo("0.1.0");
    assert!(
        sh::run(d.path(), "git", &["tag", "-a", "v0.1.0", "-m", "v0.1.0"])
            .unwrap()
            .ok()
    );
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
    )
    .unwrap();
    assert!(
        sh::run(d.path(), "git", &[
            "commit",
            "--quiet",
            "-am",
            "chore: bump"
        ])
        .unwrap()
        .ok()
    );
    // 0.1.1 is what a patch makes of v0.1.0, and not what a major makes
    let err = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Major,
        "d",
    )
    .unwrap_err();
    assert!(
        matches!(&err, PlanError::OffLevel(o)
            if o.manifest == Version::new(0, 1, 1) && o.next == Version::new(0, 2, 0)),
        "{err}"
    );
    assert!(err.to_string().contains("0.1.1"));
    assert!(err.to_string().contains("0.2.0"));
    assert!(
        err.to_string().contains("pick the level"),
        "above the tag a level may make the manifest's version: {err}"
    );
    // below the tag no level makes it, and the message does not offer one
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.0.9\"\n",
    )
    .unwrap();
    commit(d.path(), "chore: back too far");
    let err = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Patch,
        "d",
    )
    .unwrap_err();
    assert!(matches!(&err, PlanError::OffLevel(_)), "{err}");
    assert!(
        err.to_string().contains("behind the last tag 0.1.0"),
        "{err}"
    );
    assert!(!err.to_string().contains("pick the level"), "{err}");
    // and back at what a patch makes of the tag
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
    )
    .unwrap();
    commit(d.path(), "chore: at the level");
    let p = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Patch,
        "d",
    )
    .unwrap();
    assert_eq!(
        p.next,
        Version::new(0, 1, 1),
        "already at the level's version, so that is the release"
    );
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    assert!(
        sh::run(d.path(), "git", &[
            "commit",
            "--quiet",
            "-am",
            "chore: back"
        ])
        .unwrap()
        .ok()
    );
    let p = plan(
        d.path(),
        "x",
        &Markers::default(),
        "HEAD",
        Level::Major,
        "d",
    )
    .unwrap();
    assert_eq!(p.next, Version::new(0, 2, 0), "a major on 0.x is a minor");
}

#[test]
fn a_tree_without_a_manifest_is_not_plannable() {
    let d = tempfile::tempdir().unwrap();
    assert!(sh::run(d.path(), "git", &["init", "--quiet"]).unwrap().ok());
    assert!(matches!(
        plan(
            d.path(),
            "x",
            &Markers::default(),
            "HEAD",
            Level::Patch,
            "d"
        ),
        Err(PlanError::NoManifest(_))
    ));
}
