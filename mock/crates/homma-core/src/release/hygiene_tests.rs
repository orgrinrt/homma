//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The warn rows against a real repository: each fires on the case it names
//! and stays quiet on the clean one beside it.

use super::*;
use crate::release::registry::Registry;

pub(super) fn sh(d: &Path, args: &[&str]) {
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
}

pub(super) fn commit(d: &Path, path: &str, body: &str, msg: &str) {
    let at = d.join(path);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, body).unwrap();
    sh(d, &["add", path]);
    sh(d, &["commit", "-q", "-m", msg]);
}

/// A clean repo: a readme, a licence, one tagged release on `main`.
pub(super) fn clean() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let p = d.path();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["config", "tag.gpgsign", "false"],
    ] {
        sh(p, &args);
    }
    commit(p, "README.md", "x\n", "docs: readme");
    commit(p, "LICENSE", "MPL\n===================\n", "docs: licence");
    commit(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        "feat: 0.1.0",
    );
    sh(p, &["tag", "-a", "v0.1.0", "-m", "v0.1.0"]);
    d
}

pub(super) fn run(p: &Path, published: &Published) -> Vec<String> {
    run_on(p, published, "dev", "main")
}

/// The rows over `p`, the working trunk and the release branch named.
pub(super) fn run_on(p: &Path, published: &Published, trunk: &str, release: &str) -> Vec<String> {
    let tags = git::tags(p).unwrap();
    let mut versioned: Vec<(String, Version)> = tags
        .iter()
        .filter_map(|t| tag_version(t).map(|v| (t.clone(), v)))
        .collect();
    versioned.sort_by(|a, b| a.1.cmp(&b.1));
    let tracked = git::tracked_at(p, "HEAD").unwrap();
    findings(&Seen {
        root: p,
        remote: "origin",
        trunk,
        release,
        tags: &tags,
        versioned: &versioned,
        tracked: &tracked,
        published,
        riders: &[],
        manifest: Some("Cargo.toml"),
    })
    .unwrap()
    .into_iter()
    .map(|f| {
        assert_eq!(f.severity, CheckSeverity::Warn, "{}", f.id);
        format!("{} {}", f.id, f.message)
    })
    .collect()
}

pub(super) fn ids(found: &[String]) -> Vec<&str> {
    found.iter().map(|f| f.split(' ').next().unwrap()).collect()
}

#[test]
fn a_clean_repo_raises_nothing() {
    let d = clean();
    assert_eq!(run(d.path(), &Published::default()), Vec::<String>::new());
}

#[test]
fn a_stray_tag_a_stash_and_a_pushurl_are_each_named_and_the_url_is_masked() {
    let d = clean();
    let p = d.path();
    sh(p, &["tag", "-a", "archive/old", "-m", "x"]);
    std::fs::write(p.join("README.md"), "changed\n").unwrap();
    sh(p, &["stash", "-q"]);
    sh(p, &[
        "remote",
        "add",
        "origin",
        "https://example.invalid/x.git",
    ]);
    sh(p, &[
        "config",
        "remote.origin.pushurl",
        "https://user:hunter2@example.invalid/x.git",
    ]);
    let found = run(p, &Published::default());
    let ids = ids(&found);
    assert!(ids.contains(&"tag.stray"), "{found:?}");
    assert!(ids.contains(&"tree.stash"), "{found:?}");
    let push = found
        .iter()
        .find(|f| f.starts_with("remote.pushurl"))
        .unwrap();
    assert!(!push.contains("hunter2"), "{push}");
    assert!(push.contains("https://***@example.invalid"), "{push}");
}

#[test]
fn the_history_scans_find_a_token_a_conflict_an_env_file_and_a_byline() {
    let d = clean();
    let p = d.path();
    commit(
        p,
        "src/cfg.txt",
        &format!("token = ghp_{}\n", "a".repeat(36)),
        "feat: config",
    );
    commit(
        p,
        "src/merge.txt",
        "<<<<<<< ours\na\n=======\nb\n>>>>>>> theirs\n",
        "fix: merge",
    );
    commit(p, ".env", "A=1\n", "chore: env\n\nGenerated with a tool");
    let found = run(p, &Published::default());
    let ids = ids(&found);
    for id in ["hist.secret.tree", "hist.conflict", "hist.env", "hist.byline"] {
        assert!(ids.contains(&id), "{id} missing from {found:?}");
    }
    // the licence's rule of `=` is no conflict
    assert!(!found.iter().any(|f| f.contains("LICENSE")), "{found:?}");
}

#[test]
fn a_missing_readme_and_licence_and_an_em_dash_are_named() {
    let d = clean();
    let p = d.path();
    sh(p, &["rm", "-q", "README.md", "LICENSE"]);
    sh(p, &["commit", "-q", "-m", "chore: drop"]);
    commit(
        p,
        "docs/a.md",
        "one \u{2014} two \u{2014} three\n",
        "docs: a",
    );
    let found = run(p, &Published::default());
    let ids = ids(&found);
    assert!(ids.contains(&"doc.readme.exists"), "{found:?}");
    assert!(ids.contains(&"doc.licence.file"), "{found:?}");
    let dash = found.iter().find(|f| f.starts_with("doc.emdash")).unwrap();
    assert!(dash.contains("docs/a.md") && dash.contains(" 1 "), "{dash}");
}

#[test]
fn a_tag_gap_a_bump_set_twice_and_a_sandwiched_version_are_named() {
    let d = clean();
    let p = d.path();
    commit(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.3.0\"\n",
        "feat: 0.3.0",
    );
    sh(p, &["tag", "-a", "v0.3.0", "-m", "v0.3.0"]);
    commit(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        "revert",
    );
    commit(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.3.0\"\n",
        "again",
    );
    let mut published = Published::default();
    published
        .versions
        .insert((Registry::CratesIo, "x".into()), vec![
            "0.3.0".parse().unwrap(),
        ]);
    let found = run(p, &published);
    let ids = ids(&found);
    assert!(ids.contains(&"semver.gaps.tags"), "{found:?}");
    assert!(ids.contains(&"tag.bump.once"), "{found:?}");
    let sw = found
        .iter()
        .find(|f| f.starts_with("reg.sandwiched"))
        .unwrap();
    assert!(sw.contains("0.1.0"), "{sw}");
}

#[test]
fn a_yanked_slot_is_taken_and_no_hole() {
    let d = clean();
    let p = d.path();
    commit(
        p,
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
        "feat: 0.1.1",
    );
    sh(p, &["tag", "-a", "v0.1.1", "-m", "v0.1.1"]);
    let mut published = Published::default();
    let key = (Registry::Jsr, "x".to_string());
    published
        .versions
        .insert(key.clone(), vec!["0.1.1".parse().unwrap()]);
    published.yanked.insert(key, vec!["0.1.0".parse().unwrap()]);
    assert!(!ids(&run(p, &published)).contains(&"reg.sandwiched"));
}

#[test]
fn the_conflict_reader_wants_all_three_markers_in_order() {
    assert!(has_conflict("<<<<<<< a\nx\n=======\ny\n>>>>>>> b\n"));
    assert!(!has_conflict("Title\n=======\n"));
    assert!(!has_conflict(">>>>>>> b\n=======\n<<<<<<< a\n"));
}

#[test]
fn a_url_without_credentials_is_left_as_it_is() {
    assert_eq!(
        masked("https://example.invalid/x.git"),
        "https://example.invalid/x.git"
    );
    assert_eq!(masked("https://t@h/x"), "https://***@h/x");
}
