//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

fn ws() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    d
}

#[test]
fn init_writes_a_file_that_loads_and_ignores_it() {
    let d = ws();
    assert!(init(d.path(), "kenno").unwrap());
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.instance.work, "kenno");
    let ignore = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "/homma.local.toml"), "{ignore}");
}

#[test]
fn init_refuses_to_overwrite() {
    let d = ws();
    init(d.path(), "first").unwrap();
    set(d.path(), "instance.title", "kept").unwrap();
    assert!(init(d.path(), "second").is_err());
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.instance.work, "first");
    assert_eq!(parsed.instance.title.as_deref(), Some("kept"));
}

#[test]
fn init_refuses_where_there_is_no_manifest() {
    let d = tempfile::tempdir().unwrap();
    assert!(init(d.path(), "k").is_err());
    assert!(!d.path().join(LOCAL_FILE).exists());
    assert!(!d.path().join(".gitignore").exists());
}

#[test]
fn init_with_the_line_already_ignored_reports_no_change() {
    let d = ws();
    std::fs::write(d.path().join(".gitignore"), "/homma.local.toml\n").unwrap();
    assert!(!init(d.path(), "k").unwrap());
}

#[test]
fn show_without_a_file_is_none() {
    let d = ws();
    assert!(show(d.path()).unwrap().is_none());
}

#[test]
fn show_returns_the_text_as_written() {
    let d = ws();
    let text = "# mine\n[instance]\nwork = \"k\"\n";
    std::fs::write(d.path().join(LOCAL_FILE), text).unwrap();
    assert_eq!(show(d.path()).unwrap().unwrap().1, text);
}

#[test]
fn show_of_a_malformed_file_is_an_error_not_none() {
    let d = ws();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\n").unwrap();
    assert!(show(d.path()).is_err());
}

#[test]
fn set_without_a_file_refuses_and_writes_nothing() {
    let d = ws();
    assert!(set(d.path(), "instance.work", "k").is_err());
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn set_writes_through_to_the_disk() {
    let d = ws();
    init(d.path(), "k").unwrap();
    set(d.path(), "tools.dashboard.url", "https://x").unwrap();
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.tools["dashboard"]["url"].as_str(), Some("https://x"));
}

#[test]
fn a_refused_set_leaves_the_file_as_it_was() {
    let d = ws();
    init(d.path(), "k").unwrap();
    let before = std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap();
    assert!(set(d.path(), "instance.repos", "not a list").is_err());
    assert!(set(d.path(), "workspace.name", "x").is_err());
    assert_eq!(
        std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap(),
        before
    );
}

#[test]
fn set_repairs_a_file_the_manifest_load_would_refuse() {
    // The reason none of these goes through `Config`: a file missing its work
    // fails that load, and `set` is how it gets its work back.
    let d = ws();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\ntitle = \"t\"\n").unwrap();
    assert!(homma_core::Config::from_path(&d.path().join("homma.toml")).is_err());
    set(d.path(), "instance.work", "k").unwrap();
    let cfg = homma_core::Config::from_path(&d.path().join("homma.toml")).unwrap();
    assert_eq!(cfg.local.unwrap().instance.work, "k");
}
