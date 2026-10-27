//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma what` through the binary: the exit status, the two formats and the filler, run against the
//! workspace this repository is cloned into.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

/// The workspace around this repository, as the command's own acceptance set finds it.
fn workspace() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .find(|p| p.join(".shared/scripts").is_dir() && p.join("tools").is_dir())
        .expect("no workspace around this repository: the smoke test runs against its real tools")
        .to_path_buf()
}

fn what(args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("homma-engine")
        .expect("binary built")
        .arg("--dir")
        .arg(workspace())
        .arg("what")
        .arg("--no-mock")
        .args(args)
        .assert()
}

#[test]
fn an_answer_names_the_command_and_exits_zero() {
    what(&["free", "disk"])
        .success()
        .stdout(predicate::str::contains("homma disk-trim"))
        .stdout(predicate::str::contains(
            "matched: free~syn~trim (name), disk (name)",
        ));
}

#[test]
fn the_words_need_no_quotes_and_the_filler_changes_nothing() {
    let a = what(&["free", "disk"])
        .success()
        .get_output()
        .stdout
        .clone();
    let b = what(&["what", "to", "use", "for", "freeing", "the", "disk"])
        .success()
        .get_output()
        .stdout
        .clone();
    let first = |o: &[u8]| {
        String::from_utf8_lossy(o)
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(first(&a), first(&b));
}

#[test]
fn no_answer_is_a_non_zero_exit_that_says_what_was_read() {
    what(&["bake", "a", "sourdough", "loaf"])
        .failure()
        .stdout(predicate::str::contains(
            "nothing matches `bake a sourdough loaf` among",
        ));
}

#[test]
fn json_is_one_document_with_the_hits_and_the_gaps() {
    let o = Command::cargo_bin("homma-engine")
        .unwrap()
        .arg("--output")
        .arg("json")
        .arg("--dir")
        .arg(workspace())
        .args(["what", "--no-mock", "--limit", "2", "end", "a", "lost", "cargo", "run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&o).expect("one json document");
    assert_eq!(v["hits"].as_array().unwrap().len(), 2);
    assert_eq!(v["hits"][0]["name"], "stray-runs");
    assert_eq!(v["hits"][0]["command"], "homma stray-runs end");
    assert!(v["gaps"].is_array());
}

#[test]
fn a_kind_restricts_the_answer() {
    what(&["--kind", "skill", "mockspace", "round"])
        .success()
        .stdout(predicate::str::contains("skill  /work-in-mockspace"))
        .stdout(predicate::str::contains("script ").not());
}

#[test]
fn a_limit_of_nothing_is_refused_rather_than_reported_as_no_answer() {
    what(&["--limit", "0", "free", "disk"])
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--limit"));
}

#[test]
fn no_words_are_refused_by_the_parser() {
    what(&[]).failure().code(2);
}
