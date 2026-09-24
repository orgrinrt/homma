//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma capture` through the binary: finding the transcript under a home
//! directory, writing into the store, and never writing over a capture.
//!
//! What the file holds is tested beside the command, over planted transcripts.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

const SAID: &str = r#"{"type":"user","uuid":"a","timestamp":"2026-09-24T14:00:00Z","origin":{"kind":"human"},"message":{"content":"hello there"}}"#;
const MORE: &str = r#"{"type":"user","uuid":"b","timestamp":"2026-09-24T15:00:00Z","origin":{"kind":"human"},"message":{"content":"and more"}}"#;

/// A workspace with a manifest, and a home directory beside it.
fn workspace(dir: &Path) -> (PathBuf, PathBuf) {
    let root = dir.join("ws");
    let home = dir.join("home");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(root.join("homma.toml"), "[workspace]\nname = \"ws\"\n").unwrap();
    (root, home)
}

fn capture(root: &Path, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    bin()
        .env("HOME", home)
        .env("TZ", "UTC")
        .current_dir(root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "capture"])
        .args(args)
        .assert()
}

fn store(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join(".data/op-responses"))
        .map(|d| {
            d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_newest_transcript_under_home_is_read_into_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = workspace(dir.path());
    // The harness names the directory for the path it was started in, and
    // on some systems that path reaches the temporary directory through a
    // link, so both spellings are planted.
    for at in [root.clone(), root.canonicalize().unwrap()] {
        let escaped: String = at
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let projects = home.join(".claude/projects").join(escaped);
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(projects.join("sess.jsonl"), format!("{SAID}\n")).unwrap();
    }
    capture(&root, &home, &["--title", "First words"])
        .success()
        .stdout(predicate::str::contains("1 said, 0 asked"));
    let names = store(&root);
    assert_eq!(names, ["202609241400_first-words.md"]);
    let body = std::fs::read_to_string(root.join(".data/op-responses").join(&names[0])).unwrap();
    assert!(
        body.contains("source: sess\n") && body.contains("> hello there\n"),
        "{body}"
    );

    // Nothing new: nothing written, and it says so.
    capture(&root, &home, &["--title", "Again"])
        .success()
        .stdout(predicate::str::contains(
            "nothing in sess after 2026-09-24T14:00:00Z",
        ));
    assert_eq!(store(&root).len(), 1);
}

#[test]
fn a_session_by_path_and_a_store_by_flag() {
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = workspace(dir.path());
    let t = dir.path().join("other.jsonl");
    std::fs::write(&t, format!("{SAID}\n{MORE}\n")).unwrap();
    let into = dir.path().join("elsewhere");
    capture(&root, &home, &[
        "--title",
        "Later",
        "--session",
        t.to_str().unwrap(),
        "--since",
        "2026-09-24T14:30:00Z",
        "--into",
        into.to_str().unwrap(),
    ])
    .success();
    let body = std::fs::read_to_string(into.join("202609241500_later.md")).unwrap();
    assert!(
        body.contains("> and more") && !body.contains("hello there"),
        "{body}"
    );
    assert!(store(&root).is_empty());
}

#[test]
fn a_capture_already_there_is_never_written_over() {
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = workspace(dir.path());
    let t = dir.path().join("s.jsonl");
    std::fs::write(&t, format!("{SAID}\n")).unwrap();
    let file = root.join(".data/op-responses/202609241400_same.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "somebody's capture\n").unwrap();
    capture(&root, &home, &[
        "--title",
        "Same",
        "--session",
        t.to_str().unwrap(),
    ])
    .failure()
    .stderr(predicate::str::contains("never written over"));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "somebody's capture\n"
    );
}

#[test]
fn no_transcript_and_a_bad_instant_are_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = workspace(dir.path());
    capture(&root, &home, &["--title", "x"])
        .failure()
        .stderr(predicate::str::contains("--session"));
    capture(&root, &home, &["--title", "x", "--since", "last tuesday"]).failure();
    capture(&root, &home, &[]).failure();
    assert!(store(&root).is_empty());
}
