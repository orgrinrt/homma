//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma answers` through the binary: a take writes one capture and attaches the row, a second
//! take prints and writes nothing, and the hook mockspace runs reaches the same capture store.
//!
//! The launcher mockspace is run through is a stand-in that records its arguments, so the row
//! write is checked at the one place it leaves this command.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    let mut c = Command::cargo_bin("homma-engine").expect("binary built");
    c.env_remove("CLAUDE_CODE_SESSION_ID");
    c
}

/// A workspace holding one clone named `kaski`, a page directory with one console ticket answer,
/// and a launcher that appends its arguments to `args`.
fn setup(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let root = dir.join("ws");
    std::fs::create_dir_all(root.join("kaski/.git")).unwrap();
    std::fs::write(root.join("homma.toml"), "[workspace]\nname = \"ws\"\n").unwrap();
    let page = dir.join("page");
    std::fs::create_dir_all(page.join("answers")).unwrap();
    std::fs::write(
        page.join("answers/tk.json"),
        r#"{"id":"tk","key":{"ticket":"tk_one"},"kind":"ticket","where":"kaski","asks":"Which?","options":["a","b"],"choice":"b","text":"because","at":"2026-10-28T09:40:00Z"}"#,
    )
    .unwrap();
    let args = dir.join("args");
    let mock = dir.join("mock");
    std::fs::write(
        &mock,
        format!("#!/bin/sh\necho \"$@\" >> '{}'\n", args.display()),
    )
    .unwrap();
    std::fs::set_permissions(&mock, std::fs::Permissions::from_mode(0o755)).unwrap();
    (root, page, mock)
}

fn answers(root: &Path, mock: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    bin()
        .env("HOMMA_MOCK", mock)
        .current_dir(root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "answers"])
        .args(args)
        .assert()
}

fn captures(root: &Path) -> usize {
    std::fs::read_dir(root.join(".data/op-responses")).map_or(0, Iterator::count)
}

#[test]
fn the_first_take_writes_one_capture_and_the_second_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (root, page, mock) = setup(dir.path());
    let page = page.to_str().unwrap();
    answers(&root, &mock, &["take", "--dir", page])
        .success()
        .stdout("took 1 answer\n");
    assert_eq!(captures(&root), 1);
    let log = std::fs::read_to_string(dir.path().join("args")).unwrap();
    assert!(
        log.starts_with(
            "registry-append ticket::tk_one answers 2026-10-28T09:40:00Z: .data/op-responses/"
        ),
        "{log}"
    );
    assert!(log.trim_end().ends_with("--local"), "{log}");
    answers(&root, &mock, &["take", "--dir", page])
        .success()
        .stdout("");
    assert_eq!(captures(&root), 1);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("args"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn json_output_names_the_capture_and_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let (root, page, mock) = setup(dir.path());
    let out = bin()
        .env("HOMMA_MOCK", &mock)
        .current_dir(&root)
        .args(["-c", root.join("homma.toml").to_str().unwrap(), "--output", "json", "answers"])
        .args(["take", "--dir", page.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ticket::tk_one"));
    let _ = out;
}

#[test]
fn a_take_with_no_source_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (root, _page, mock) = setup(dir.path());
    answers(&root, &mock, &["take"]).failure();
}

#[test]
fn the_placed_hook_takes_words_and_declines_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (root, _page, mock) = setup(dir.path());
    answers(&root, &mock, &["hook"]).success();
    let hook = root.join(".mockspace/hooks/registry-append");
    assert!(hook.is_file());
    let run = |field: &str, value: &str| {
        std::process::Command::new(&hook)
            .current_dir(root.join("kaski"))
            .env("HOMMA_MOCK", &mock)
            .env("MOCKSPACE_REPO_ROOT", root.join("kaski"))
            .args(["ticket::tk_one", field, value])
            .status()
            .unwrap()
            .code()
    };
    assert_eq!(run("answers", "2026-10-28T09:41:00Z: yes please"), Some(0));
    assert_eq!(captures(&root), 1);
    assert_eq!(run("title", "2026-10-28T09:41:00Z: x"), Some(3));
    answers(&root, &mock, &["hook", "--remove"]).success();
    assert!(!hook.exists());
}

#[test]
fn a_hook_whose_row_cannot_be_written_exits_one_leaves_no_capture_and_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let (root, _page, _mock) = setup(dir.path());
    // A launcher that refuses until the row "exists", which is the file `ready`.
    let ready = dir.path().join("ready");
    let mock = dir.path().join("refusing-mock");
    std::fs::write(
        &mock,
        format!(
            "#!/bin/sh\n[ -e '{}' ] || {{ echo 'no such row' >&2; exit 2; }}\necho \"$@\" >> '{}'\n",
            ready.display(),
            dir.path().join("args").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&mock, std::fs::Permissions::from_mode(0o755)).unwrap();
    answers(&root, &mock, &["hook"]).success();
    let hook = root.join(".mockspace/hooks/registry-append");
    let run = || {
        std::process::Command::new(&hook)
            .current_dir(root.join("kaski"))
            .env("HOMMA_MOCK", &mock)
            .env("MOCKSPACE_REPO_ROOT", root.join("kaski"))
            .args(["ticket::tk_one", "answers", "2026-10-28T09:41:00Z: yes please"])
            .output()
            .unwrap()
    };
    let failed = run();
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("no such row"));
    assert_eq!(
        captures(&root),
        0,
        "a failed append leaves nothing to be dropped as recorded"
    );
    std::fs::write(&ready, "").unwrap();
    assert_eq!(run().status.code(), Some(0));
    assert_eq!(captures(&root), 1);
    let log = std::fs::read_to_string(dir.path().join("args")).unwrap();
    assert!(
        log.contains("ticket::tk_one answers 2026-10-28T09:41:00Z: "),
        "{log}"
    );
}
