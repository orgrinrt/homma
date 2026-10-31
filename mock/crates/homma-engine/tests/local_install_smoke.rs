//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma local install` and `homma local list`, through the binary.

mod support;

use std::path::Path;

use support::bin;

const KASKI: &str = "[instance]\nwork = \"kaski\"\ntitle = \"the colony sim\"\n";
const KENNO: &str = "[instance]\nwork = \"kenno\"\n";

fn workspace() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    let store = d.path().join(".shared/local");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("kaski.toml"), KASKI).unwrap();
    std::fs::write(store.join("kenno.toml"), KENNO).unwrap();
    d
}

fn homma(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    bin().arg("--dir").arg(dir).arg("local").args(args).assert()
}

#[test]
fn a_fresh_clone_inherits_the_named_config_in_one_command() {
    let d = workspace();
    homma(d.path(), &["install", "kaski"]).success();
    let text = std::fs::read_to_string(d.path().join("homma.local.toml")).unwrap();
    assert!(
        text.contains("project = \"kaski\"") && !text.contains("the colony sim"),
        "{text}"
    );
    let shown = homma(d.path(), &["show", "--output", "json"]).success();
    let json: serde_json::Value = serde_json::from_slice(&shown.get_output().stdout).unwrap();
    assert_eq!(json["instance"]["work"], "kaski");
    assert_eq!(json["instance"]["title"], "the colony sim");
    // The written text, and the document in force in words.
    let written = homma(d.path(), &["show"]).success();
    assert!(String::from_utf8_lossy(&written.get_output().stdout).contains("project = \"kaski\""));
    let effective = homma(d.path(), &["show", "--effective"]).success();
    let words = String::from_utf8_lossy(&effective.get_output().stdout).into_owned();
    assert!(words.contains("title = \"the colony sim\""), "{words}");
}

#[test]
fn list_says_which_is_installed_in_the_document() {
    let d = workspace();
    homma(d.path(), &["install", "kenno"]).success();
    let out = bin()
        .arg("--dir")
        .arg(d.path())
        .args(["--output", "json", "local", "list"])
        .assert()
        .success();
    let rows: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let installed: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["installed"] == true)
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(installed, ["kenno"]);
    assert_eq!(rows.as_array().unwrap().len(), 2);
}

#[test]
fn list_in_words_marks_the_installed_name() {
    let d = workspace();
    homma(d.path(), &["install", "kaski"]).success();
    let out = homma(d.path(), &["list"]).success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert_eq!(text.lines().collect::<Vec<_>>(), [
        "kaski  installed",
        "kenno"
    ]);
}

#[test]
fn another_works_file_is_refused_with_a_failing_exit() {
    let d = workspace();
    homma(d.path(), &["install", "kaski"]).success();
    homma(d.path(), &["install", "kenno"])
        .failure()
        .stderr(predicates::str::contains("--replace"));
    homma(d.path(), &["install", "kenno", "--replace"]).success();
    assert!(
        std::fs::read_to_string(d.path().join("homma.local.toml"))
            .unwrap()
            .contains("project = \"kenno\"")
    );
    assert!(
        std::fs::read_to_string(d.path().join("homma.local.toml.replaced"))
            .unwrap()
            .contains("project = \"kaski\"")
    );
}

#[test]
fn list_without_a_store_reports_and_exits_one() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    homma(d.path(), &["list"]).code(1);
}

#[test]
fn list_without_a_store_prints_an_empty_list_in_the_document_and_exits_one() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    let out = bin()
        .arg("--dir")
        .arg(d.path())
        .args(["--output", "json", "local", "list"])
        .assert()
        .code(1);
    let rows: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(rows, serde_json::json!([]));
}

#[test]
fn show_prints_a_file_whose_project_cannot_be_read_and_the_merged_view_fails() {
    let d = workspace();
    std::fs::write(
        d.path().join("homma.local.toml"),
        "[instance]\nproject = \"gone\"\n",
    )
    .unwrap();
    let shown = homma(d.path(), &["show"]).success();
    assert!(String::from_utf8_lossy(&shown.get_output().stdout).contains("project = \"gone\""));
    homma(d.path(), &["show", "--effective"])
        .failure()
        .stderr(predicates::str::contains("`gone`"));
}
