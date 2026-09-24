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

/// The binary, outside any agent session whatever shell runs the tests.
fn bin() -> Command {
    let mut c = Command::cargo_bin("homma-engine").expect("binary built");
    c.env_remove("CLAUDE_CODE_SESSION_ID");
    c
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

#[cfg(unix)]
#[test]
fn the_newest_transcript_under_home_is_read_into_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let (real_root, home) = workspace(dir.path());
    // The workspace is reached through a link made here, so the real path and
    // the spelling the run is given always differ, whatever the platform's
    // temporary directory is.
    let root = dir.path().join("through-a-link");
    std::os::unix::fs::symlink(&real_root, &root).unwrap();
    let escaped = |at: &Path| -> PathBuf {
        let name: String = at
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        home.join(".claude/projects").join(name)
    };
    // The harness names the directory for the real path, links resolved, so
    // the spelling through the link holds a decoy that must not be read.
    let real = escaped(&root.canonicalize().unwrap());
    let linked = escaped(&root);
    assert_ne!(linked, real);
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::write(linked.join("decoy.jsonl"), format!("{MORE}\n")).unwrap();
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("sess.jsonl"), format!("{SAID}\n")).unwrap();
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
        .stdout(predicate::str::contains("nothing in sess after a"));
    assert_eq!(store(&root).len(), 1);

    // The session goes on, and the report is a document when asked for one.
    std::fs::write(real.join("sess.jsonl"), format!("{SAID}\n{MORE}\n")).unwrap();
    let out = bin()
        .env("HOME", &home)
        .env("TZ", "UTC")
        .current_dir(&root)
        .args(["--output", "json", "-c", root.join("homma.toml").to_str().unwrap()])
        .args(["capture", "--title", "More"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("a json document");
    assert_eq!(v["session"], "sess");
    assert_eq!(v["said"], 1);
    assert_eq!(v["asked"], 0);
    assert_eq!(v["after"], "a");
    assert!(
        v["written"]
            .as_str()
            .is_some_and(|p| p.ends_with("202609241500_more.md")),
        "{v}"
    );
}

#[test]
fn the_session_the_command_runs_inside_is_read_and_the_store_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    let (root, home) = workspace(dir.path());
    std::fs::write(
        root.join("homma.toml"),
        "[workspace]\nname = \"ws\"\n\n[paths]\ncaptures = \"notes/said\"\n",
    )
    .unwrap();
    let real = root.canonicalize().unwrap();
    let name: String = real
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let own = home.join(".claude/projects").join(name);
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("running.jsonl"), format!("{SAID}\n")).unwrap();
    // A headless session wrote a newer transcript beside it.
    std::fs::write(own.join("headless.jsonl"), format!("{MORE}\n")).unwrap();
    let f = std::fs::File::options()
        .write(true)
        .open(own.join("running.jsonl"))
        .unwrap();
    f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
        .unwrap();
    bin()
        .env("HOME", &home)
        .env("TZ", "UTC")
        .env("CLAUDE_CODE_SESSION_ID", "running")
        .current_dir(&root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "capture"])
        .args(["--title", "Inside"])
        .assert()
        .success();
    let body = std::fs::read_to_string(root.join("notes/said/202609241400_inside.md"))
        .expect("written there");
    assert!(body.contains("source: running\n"), "{body}");
    assert!(store(&root).is_empty());
    // The control: outside a session, the newest is the headless one.
    capture(&root, &home, &["--title", "Outside"]).success();
    assert!(root.join("notes/said/202609241500_outside.md").is_file());
}

#[test]
fn a_session_by_path_needs_no_home() {
    let dir = tempfile::tempdir().unwrap();
    let (root, _) = workspace(dir.path());
    let t = dir.path().join("s.jsonl");
    std::fs::write(&t, format!("{SAID}\n")).unwrap();
    bin()
        .env_remove("HOME")
        .env("TZ", "UTC")
        .current_dir(&root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "capture"])
        .args(["--title", "No home", "--session", t.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(store(&root), ["202609241400_no-home.md"]);
    // The control: by id, it does need one, and says what to do instead.
    bin()
        .env_remove("HOME")
        .current_dir(&root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "capture"])
        .args(["--title", "No home", "--session", "s"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no HOME"));
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
    capture(&root, &home, &["--title", "x", "--since", "last tuesday"])
        .failure()
        .stderr(predicate::str::contains("--since"));
    capture(&root, &home, &[])
        .failure()
        .stderr(predicate::str::contains("--title"));
    assert!(store(&root).is_empty());
}
