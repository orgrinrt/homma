//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The history scan: a manifest naming it or not, and a flagged history, a
//! clean one and a scan that could not run, each read as the run reads them.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::commands::{history_refusal, history_scan_of};

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A clone with one commit whose message is `message`.
fn history(dir: &Path, message: &str) -> PathBuf {
    let root = dir.join("x");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q", "-b", "dev"]);
    git(&root, &["config", "user.email", "t@t"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    std::fs::write(root.join("a"), "a\n").unwrap();
    git(&root, &["add", "a"]);
    git(&root, &["commit", "-qm", message]);
    root
}

/// A stand-in for the workspace's scanner, with its exit codes: 1 where the
/// history carries a trailer naming an agent, 2 where it cannot enter the clone,
/// 0 otherwise. It reads the clone
/// it is given, never its working directory, as the real one does.
fn scanner(dir: &Path, body: &str) -> PathBuf {
    let at = dir.join("scan");
    crate::cmd::planted::script(&at, &format!("#!/bin/sh\n{body}\n"), 0o755);
    at
}

const FLAGS_A_TRAILER: &str = "git -C \"$1\" rev-parse --git-dir >/dev/null 2>&1 || exit 2; if git -C \"$1\" log --all --format=%B | grep -q '^Co-Authored-By: Claude'; then \
     echo \"trailer: Co-Authored-By: Claude\"; exit 1; fi; echo clean; exit 0";

#[test]
fn a_flagged_history_is_refused_with_what_the_scan_said_and_the_rehouse() {
    // Mutation: the scan's exit read as advice, which releases the history it
    // flagged.
    let dir = tempfile::tempdir().unwrap();
    let root = history(
        dir.path(),
        "fix: a thing\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    );
    let scan = scanner(dir.path(), FLAGS_A_TRAILER);
    let why = history_refusal(&scan, &root).expect("a flagged history is refused");
    assert!(why.contains("carries attribution"), "{why}");
    assert!(why.contains("trailer: Co-Authored-By: Claude"), "{why}");
    assert!(why.contains("rehouse"), "{why}");
}

#[test]
fn a_clean_history_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let root = history(dir.path(), "fix: a thing");
    let scan = scanner(dir.path(), FLAGS_A_TRAILER);
    assert_eq!(history_refusal(&scan, &root), None);
}

#[test]
fn a_root_given_relative_is_scanned_where_it_is() {
    // Mutation: the root handed on as given, which the scan, run from inside
    // it, reads relative to itself and cannot enter, refusing a clean history.
    let dir = tempfile::tempdir_in(".").unwrap();
    history(dir.path(), "fix: a thing");
    let scan = scanner(dir.path(), FLAGS_A_TRAILER);
    let name = dir.path().file_name().unwrap();
    let relative = Path::new(name).join("x");
    assert!(relative.is_relative());
    assert_eq!(history_refusal(&scan, &relative), None);
}

#[test]
fn a_scan_that_could_not_scan_or_could_not_run_is_a_refusal() {
    // Mutation: only exit 1 refused, which takes a scan that never read the
    // history for a clean one.
    let dir = tempfile::tempdir().unwrap();
    let root = history(dir.path(), "fix: a thing");
    let broken = scanner(dir.path(), "echo 'the engine did not load' >&2; exit 2");
    let why = history_refusal(&broken, &root).expect("an unscanned history is refused");
    assert!(why.contains("could not be scanned"), "{why}");
    assert!(why.contains("the engine did not load"), "{why}");
    let missing = dir.path().join("nowhere");
    assert!(history_refusal(&missing, &root).is_some());
}

#[test]
fn the_scan_is_read_from_the_manifest_and_one_named_and_not_there_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let at = dir.path().join("homma.toml");
    std::fs::write(&at, "[workspace]\nname = \"w\"\n").unwrap();
    assert_eq!(history_scan_of(&at).unwrap(), None);
    std::fs::write(
        &at,
        "[workspace]\nname = \"w\"\n\n[paths]\nhistory_scan = \"scan\"\n",
    )
    .unwrap();
    let err = history_scan_of(&at).unwrap_err().to_string();
    assert!(err.contains("history_scan"), "{err}");
    scanner(dir.path(), "exit 0");
    assert_eq!(
        history_scan_of(&at).unwrap().as_deref(),
        Some(
            std::path::absolute(dir.path().join("scan"))
                .unwrap()
                .as_path()
        )
    );
}
