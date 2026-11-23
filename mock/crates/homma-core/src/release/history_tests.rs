//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The step rules, which steps a tag may stand for, which tag released which
//! change, the tree matching that survives a real rewrite, and which
//! manifest carries a repo's version.

use std::path::Path;

use super::*;

const TRUNKS: &[&str] = &["dev", "main"];

fn ok(a: Option<&str>, b: &str) {
    let (good, why) = sensible(a, b);
    assert!(good, "{a:?} -> {b} rejected: {why}");
}

fn no(a: Option<&str>, b: &str) {
    let (good, why) = sensible(a, b);
    assert!(!good, "{a:?} -> {b} accepted as {why}");
}

#[test]
fn the_three_legal_increments() {
    ok(Some("1.2.3"), "1.2.4");
    ok(Some("1.2.3"), "1.3.0");
    ok(Some("1.2.3"), "2.0.0");
}

#[test]
fn skipping_is_caught() {
    no(Some("0.0.3"), "0.0.5");
    no(Some("0.1.0"), "0.1.2");
    no(Some("0.1.10"), "0.3.0");
}

#[test]
fn backwards_is_caught() {
    no(Some("0.1.0"), "0.0.9");
    no(Some("2.0.6"), "2.0.5");
}

#[test]
fn a_skip_names_the_legal_targets() {
    let (_, why) = sensible(Some("0.0.3"), "0.0.5");
    assert!(why.contains("0.0.4"), "{why}");
}

#[test]
fn first_version_is_always_fine() {
    ok(None, "0.1.0");
    ok(None, "7.7.7");
}

#[test]
fn the_marker_parses_and_others_do_not() {
    assert_eq!(design_round("0.0.0-d01"), Some(1));
    assert_eq!(design_round("0.0.0-d12"), Some(12));
    for other in ["0.1.0", "0.0.1", "1.0.0-alpha", "0.0.0", ""] {
        assert_eq!(design_round(other), None, "{other}");
    }
}

#[test]
fn rounds_advance_by_one() {
    ok(Some("0.0.0-d01"), "0.0.0-d02");
    no(Some("0.0.0-d01"), "0.0.0-d03");
    no(Some("0.0.0-d03"), "0.0.0-d02");
}

#[test]
fn the_design_phase_exits_at_0_0_1_and_nowhere_else() {
    ok(Some("0.0.0-d04"), "0.0.1");
    for wrong in ["0.1.0", "1.0.0", "0.0.2"] {
        no(Some("0.0.0-d04"), wrong);
    }
}

#[test]
fn you_cannot_go_back_into_the_design_phase() {
    no(Some("0.1.0"), "0.0.0-d01");
}

fn steps(versions: &[&str]) -> Vec<Step> {
    let mut out = Vec::new();
    let mut prev: Option<String> = None;
    for (i, v) in versions.iter().enumerate() {
        out.push(Step {
            sha:     format!("sha{i:05}"),
            date:    "2026-01-01".into(),
            subject: format!("to {v}"),
            from:    prev.clone(),
            to:      v.to_string(),
        });
        prev = Some(v.to_string());
    }
    out
}

fn tos(s: &[Step]) -> Vec<String> {
    s.iter().map(|s| s.to.clone()).collect()
}

#[test]
fn the_placeholder_is_not_a_release() {
    assert_eq!(tos(&releasable(&steps(&["0.0.0", "0.0.1"]))), ["0.0.1"]);
}

#[test]
fn everything_before_a_backwards_step_is_scaffold() {
    assert_eq!(
        tos(&releasable(&steps(&["0.1.0", "0.0.0", "0.0.1", "0.0.2"]))),
        ["0.0.1", "0.0.2"]
    );
}

#[test]
fn a_forward_skip_is_flagged_but_still_releasable() {
    assert_eq!(tos(&releasable(&steps(&["0.0.1", "0.0.3"]))), [
        "0.0.1", "0.0.3"
    ]);
}

#[test]
fn the_design_phase_is_releasable_and_its_exit_is_too() {
    assert_eq!(
        tos(&releasable(&steps(&["0.0.0-d01", "0.0.0-d02", "0.0.1"]))),
        ["0.0.0-d01", "0.0.0-d02", "0.0.1"]
    );
}

#[test]
fn an_ordinary_line_keeps_every_step() {
    assert_eq!(
        releasable(&steps(&["0.0.1", "0.0.2", "0.1.0", "1.0.0"])).len(),
        4
    );
}

#[test]
fn nothing_survives_a_line_that_is_only_scaffold() {
    assert!(releasable(&steps(&["0.1.0", "0.0.0"])).is_empty());
    assert!(releasable(&[]).is_empty());
}

fn sh(d: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(d)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@example.invalid"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["config", "tag.gpgsign", "false"],
    ] {
        sh(d.path(), &args);
    }
    d
}

fn set_version(d: &Path, v: &str, msg: &str) {
    std::fs::write(
        d.join("Cargo.toml"),
        format!("[package]\nname = \"x\"\nversion = \"{v}\"\n"),
    )
    .unwrap();
    sh(d, &["add", "Cargo.toml"]);
    sh(d, &["commit", "-q", "-m", msg]);
}

/// Three versions on `main`, `v0.0.2` tagged at its bump.
fn rewritten() -> tempfile::TempDir {
    let d = init();
    for v in ["0.0.1", "0.0.2", "0.0.3"] {
        set_version(d.path(), v, &format!("release {v}"));
        if v == "0.0.2" {
            sh(d.path(), &["tag", "-a", "v0.0.2", "-m", "0.0.2"]);
        }
    }
    d
}

#[test]
fn timeline_reads_every_change() {
    let d = rewritten();
    assert_eq!(tos(&timeline(d.path(), "Cargo.toml", TRUNKS).unwrap()), [
        "0.0.1", "0.0.2", "0.0.3"
    ]);
}

#[test]
fn the_tag_is_on_branch_before_the_rewrite() {
    let d = rewritten();
    assert!(orphans(d.path(), TRUNKS).unwrap().is_empty());
}

#[test]
fn after_a_rewrite_the_tag_is_orphaned_and_relocatable() {
    let d = rewritten();
    let before = sh(d.path(), &["rev-list", "-n1", "v0.0.2"]);
    sh(d.path(), &[
        "-c",
        "gc.auto=0",
        "filter-branch",
        "-f",
        "--msg-filter",
        "sed 's/release/shipped/'",
        "--",
        "--all",
    ]);
    let found = orphans(d.path(), TRUNKS).unwrap();
    assert_eq!(found.len(), 1, "the rewrite orphans exactly the one tag");
    let o = &found[0];
    assert!(
        o.recoverable(),
        "no tree match for a tag whose content still exists"
    );
    assert_eq!(o.matched, Matched::Tree);
    let target = retarget(d.path(), o, "2026-11-23T00:00:00Z").unwrap();
    assert!(
        orphans(d.path(), TRUNKS).unwrap().is_empty(),
        "still orphaned"
    );
    let after = sh(d.path(), &["show", "v0.0.2:Cargo.toml"]);
    assert!(after.contains("version = \"0.0.2\""), "{after}");
    assert!(!before.starts_with(&target), "the rewrite is the premise");
    let journal = sh(d.path(), &["rev-parse", "--git-path", JOURNAL]);
    let undo = std::fs::read_to_string(d.path().join(journal)).unwrap();
    assert!(
        undo.starts_with(&format!("git tag -f v0.0.2 {}", &before[.. 9])),
        "{undo}"
    );
}

#[test]
fn a_tag_with_two_matching_commits_is_refused_rather_than_guessed() {
    let o = Orphan {
        tag:        "v1".into(),
        old_sha:    "aaaaaaaaa".into(),
        tree:       "bbbbbbbbb".into(),
        candidates: vec!["ccccccccc".into(), "ddddddddd".into()],
        held_by:    vec![],
        matched:    Matched::Tree,
    };
    let d = init();
    assert!(matches!(
        retarget(d.path(), &o, "now"),
        Err(Refused::Ambiguous(2))
    ));
    let journal = sh(d.path(), &["rev-parse", "--git-path", JOURNAL]);
    assert!(
        !d.path().join(journal).exists(),
        "nothing journalled for a refusal"
    );
}

/// `0.0.1` bumped on `dev`, two commits after it, then the release merge to
/// `main` tagged `0.0.1`; the checkout back on `dev`.
fn merged() -> (tempfile::TempDir, String, String) {
    let d = init();
    let p = d.path();
    std::fs::write(p.join("README.md"), "start\n").unwrap();
    sh(p, &["add", "README.md"]);
    sh(p, &["commit", "-q", "-m", "initial"]);
    sh(p, &["switch", "-q", "-c", "dev"]);
    set_version(p, "0.0.1", "the version moves");
    let bump = sh(p, &["rev-parse", "HEAD"])[.. 9].to_string();
    for n in ["a", "b"] {
        std::fs::write(p.join(n), n).unwrap();
        sh(p, &["add", n]);
        sh(p, &["commit", "-q", "-m", &format!("then {n}")]);
    }
    sh(p, &["switch", "-q", "main"]);
    sh(p, &[
        "merge",
        "-q",
        "--no-ff",
        "dev",
        "-m",
        "release: 0.0.1",
    ]);
    sh(p, &["tag", "-a", "0.0.1", "-m", "0.0.1"]);
    let merge = sh(p, &["rev-parse", "HEAD"])[.. 9].to_string();
    sh(p, &["switch", "-q", "dev"]);
    (d, bump, merge)
}

#[test]
fn the_premise_the_tag_is_not_at_the_bump() {
    let (d, bump, merge) = merged();
    assert_ne!(bump, merge);
    assert_eq!(tags_by_commit(d.path(), TRUNKS).unwrap().get(&bump), None);
}

#[test]
fn the_bump_counts_as_released() {
    let (d, ..) = merged();
    let s = timeline(d.path(), "Cargo.toml", TRUNKS).unwrap();
    assert_eq!(tos(&s), ["0.0.1"]);
    assert_eq!(released_at(d.path(), &s, TRUNKS).unwrap()[&s[0].sha], [
        "0.0.1"
    ]);
}

#[test]
fn a_later_bump_is_not_released_by_an_earlier_tag() {
    let (d, ..) = merged();
    set_version(d.path(), "0.0.2", "the version moves again");
    let s = timeline(d.path(), "Cargo.toml", TRUNKS).unwrap();
    let rel = released_at(d.path(), &s, TRUNKS).unwrap();
    assert_eq!(tos(&s), ["0.0.1", "0.0.2"]);
    assert_eq!(rel[&s[0].sha], ["0.0.1"], "0.0.1 is released");
    assert!(rel[&s[1].sha].is_empty(), "0.0.2 must not borrow the tag");
}

#[test]
fn the_tag_lands_by_ancestry_and_not_by_position_in_the_list() {
    let (d, ..) = merged();
    let p = d.path();
    for v in ["0.0.2", "0.0.3"] {
        set_version(p, v, &format!("the version moves to {v}"));
    }
    sh(p, &["switch", "-q", "main"]);
    sh(p, &[
        "merge",
        "-q",
        "--no-ff",
        "dev",
        "-m",
        "release: 0.0.3",
    ]);
    sh(p, &["tag", "-a", "0.0.3", "-m", "0.0.3"]);
    sh(p, &["switch", "-q", "dev"]);
    let s = timeline(p, "Cargo.toml", TRUNKS).unwrap();
    assert_eq!(tos(&s), ["0.0.1", "0.0.2", "0.0.3"]);
    let forwards = released_at(p, &s, TRUNKS).unwrap();
    let mut rev = s.clone();
    rev.reverse();
    let backwards = released_at(p, &rev, TRUNKS).unwrap();
    assert_eq!(
        forwards, backwards,
        "a tag moved when the list was reordered"
    );
    assert_eq!(backwards[&s[2].sha], ["0.0.3"]);
    assert!(
        backwards[&s[1].sha].is_empty(),
        "0.0.2 was not released by 0.0.3"
    );
}

#[test]
fn a_tag_that_is_not_a_version_is_not_a_release() {
    let (d, ..) = merged();
    sh(d.path(), &[
        "tag",
        "-a",
        "archive/some-old-branch",
        "-m",
        "x",
    ]);
    let s = timeline(d.path(), "Cargo.toml", TRUNKS).unwrap();
    assert_eq!(released_at(d.path(), &s, TRUNKS).unwrap()[&s[0].sha], [
        "0.0.1"
    ]);
}

#[test]
fn the_timeline_reads_the_trunks_and_not_the_checkout() {
    let (d, ..) = merged();
    sh(d.path(), &["switch", "-q", "-c", "feat/somewhere-else"]);
    set_version(d.path(), "9.9.9", "a version that never shipped");
    assert_eq!(tos(&timeline(d.path(), "Cargo.toml", TRUNKS).unwrap()), [
        "0.0.1"
    ]);
}

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let at = d.path().join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, body).unwrap();
    }
    d
}

pub(super) const ROOT_IS_A_PACKAGE: &str = "[package]\nname = \"plain\"\nversion = \"1.2.3\"\n";
pub(super) const VIRTUAL_ROOT: &str =
    "[workspace]\nmembers = [\"launcher\"]\nresolver = \"3\"\n\n[profile.release]\nopt-level = 3\n";
pub(super) const LAUNCHER: &str =
    "[package]\nname = \"the-tool\"\nversion = \"0.0.1\"\npublish = true\n";
pub(super) const INHERITING_ROOT: &str = "[workspace]\nmembers = [\".\", \"sibling\", \"helper\"]\n\n[workspace.package]\nversion = \"0.4.0\"\nedition = \"2024\"\n\n[package]\nname = \"core-crate\"\nversion.workspace = true\n";
pub(super) const INHERITING_MEMBER: &str =
    "[package]\nname = \"sibling\"\nversion.workspace = true\n";
pub(super) const PRIVATE_MEMBER: &str =
    "[package]\nname = \"helper\"\nversion.workspace = true\npublish = false\n";

#[test]
fn a_plain_package_root_is_read_directly() {
    let d = tree(&[("Cargo.toml", ROOT_IS_A_PACKAGE)]);
    assert_eq!(manifest_for(d.path(), None), Ok(Some("Cargo.toml".into())));
}

#[test]
fn a_virtual_root_falls_through_to_its_one_versioned_member() {
    let d = tree(&[("Cargo.toml", VIRTUAL_ROOT), ("launcher/Cargo.toml", LAUNCHER)]);
    assert_eq!(
        manifest_for(d.path(), None),
        Ok(Some("launcher/Cargo.toml".into()))
    );
}

#[test]
fn several_versioned_members_is_reported_rather_than_guessed() {
    let d = tree(&[
        (
            "Cargo.toml",
            &VIRTUAL_ROOT.replace("[\"launcher\"]", "[\"launcher\", \"other\"]"),
        ),
        ("launcher/Cargo.toml", LAUNCHER),
        (
            "other/Cargo.toml",
            &LAUNCHER.replace("the-tool", "other-tool"),
        ),
    ]);
    assert_eq!(
        manifest_for(d.path(), None),
        Err(vec![
            "launcher/Cargo.toml".to_string(),
            "other/Cargo.toml".to_string()
        ])
    );
}

#[test]
fn an_override_wins_over_all_of_it() {
    let d = tree(&[("Cargo.toml", VIRTUAL_ROOT), ("launcher/Cargo.toml", LAUNCHER)]);
    assert_eq!(
        manifest_for(d.path(), Some("somewhere/else.toml")),
        Ok(Some("somewhere/else.toml".into()))
    );
}

#[test]
fn a_member_inheriting_a_version_is_not_a_timeline_of_its_own() {
    let d = tree(&[("Cargo.toml", INHERITING_ROOT), ("sibling/Cargo.toml", INHERITING_MEMBER)]);
    assert!(versioned_members(d.path()).is_empty());
    assert_eq!(manifest_for(d.path(), None), Ok(Some("Cargo.toml".into())));
}

#[test]
fn a_repo_with_no_manifest_at_all() {
    let d = tree(&[("README.md", "hi")]);
    assert_eq!(manifest_for(d.path(), None), Ok(None));
}

fn named_versions(d: &Path) -> Vec<(String, String)> {
    packages::crates(d)
        .into_iter()
        .filter_map(|c| c.version.map(|v| (c.name, v)))
        .collect()
}

#[test]
fn a_plain_package() {
    let d = tree(&[("Cargo.toml", ROOT_IS_A_PACKAGE)]);
    assert_eq!(named_versions(d.path()), [("plain".into(), "1.2.3".into())]);
}

#[test]
fn a_virtual_root_publishes_its_member_and_not_itself() {
    let d = tree(&[("Cargo.toml", VIRTUAL_ROOT), ("launcher/Cargo.toml", LAUNCHER)]);
    assert_eq!(named_versions(d.path()), [(
        "the-tool".into(),
        "0.0.1".into()
    )]);
}

#[test]
fn an_inherited_version_resolves_to_the_root() {
    let d = tree(&[
        ("Cargo.toml", INHERITING_ROOT),
        ("sibling/Cargo.toml", INHERITING_MEMBER),
        ("helper/Cargo.toml", PRIVATE_MEMBER),
    ]);
    let found = named_versions(d.path());
    assert!(found.contains(&("core-crate".into(), "0.4.0".into())));
    assert!(found.contains(&("sibling".into(), "0.4.0".into())));
}

#[test]
fn publish_false_is_not_a_crate_the_registry_owes_anything() {
    let d = tree(&[
        ("Cargo.toml", INHERITING_ROOT),
        ("sibling/Cargo.toml", INHERITING_MEMBER),
        ("helper/Cargo.toml", PRIVATE_MEMBER),
    ]);
    assert!(!named_versions(d.path()).iter().any(|(n, _)| n == "helper"));
}

#[test]
fn the_root_is_never_counted_twice() {
    let d = tree(&[("Cargo.toml", INHERITING_ROOT), ("sibling/Cargo.toml", INHERITING_MEMBER)]);
    let names: Vec<String> = named_versions(d.path())
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let mut unique = names.clone();
    unique.dedup();
    assert_eq!(names.len(), unique.len());
    assert_eq!(names.len(), 2);
}

#[test]
fn a_member_with_no_version_anywhere_is_skipped() {
    let d = tree(&[
        ("Cargo.toml", VIRTUAL_ROOT),
        ("launcher/Cargo.toml", "[package]\nname = \"the-tool\"\n"),
    ]);
    assert!(named_versions(d.path()).is_empty());
}

#[test]
fn a_glob_member_expands() {
    let d = tree(&[
        (
            "Cargo.toml",
            &VIRTUAL_ROOT.replace("[\"launcher\"]", "[\"crates/*\"]"),
        ),
        ("crates/a/Cargo.toml", &LAUNCHER.replace("the-tool", "a")),
        ("crates/b/Cargo.toml", &LAUNCHER.replace("the-tool", "b")),
    ]);
    let mut names: Vec<String> = named_versions(d.path())
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    names.sort();
    assert_eq!(names, ["a", "b"]);
}
