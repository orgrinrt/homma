//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `[[status.inject]]` end to end, through the binary.
//!
//! The unit tests hold each half: the schema, the anchoring, the runner, the
//! render. None of them can say the halves are wired to each other, and the
//! wiring is where this would go wrong silently, by parsing a table nothing
//! ever reads and printing a status that looks exactly as it did before.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

/// A workspace whose manifest carries `extra`, and a `tools/` beside it.
fn workspace(extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("homma.toml"),
        format!("content_repo = \"c\"\n\n[workspace]\nname = \"w\"\n\n{extra}"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("tools")).unwrap();
    dir
}

/// An executable script at `tools/<name>`.
fn tool(dir: &tempfile::TempDir, name: &str, body: &str) {
    let path = dir.path().join("tools").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn a_declared_tools_output_lands_in_the_status() {
    let dir = workspace("[[status.inject]]\ntool = \"tools/context\"\n");
    tool(&dir, "context", "printf '450733 of 1000000\\n'");

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("context:"))
        .stdout(predicate::str::contains("  450733 of 1000000"));
}

#[test]
fn a_relative_tool_path_is_found_from_anywhere() {
    // The whole point of anchoring against the workspace root rather than the
    // process's directory. Run from somewhere else entirely, the tool still
    // resolves, and a `--dir` run is how the launcher invokes every command.
    let dir = workspace("[[status.inject]]\ntool = \"tools/context\"\n");
    tool(&dir, "context", "printf 'found\\n'");
    let elsewhere = tempfile::tempdir().unwrap();

    bin()
        .current_dir(elsewhere.path())
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("found"));
}

#[test]
fn the_tool_runs_in_the_workspace_root() {
    // So a tool's own relative paths mean what they mean when it is run there
    // by hand, which is how every one of these is written and tested.
    let dir = workspace("[[status.inject]]\ntool = \"tools/where\"\n");
    tool(&dir, "where", "cat marker");
    std::fs::write(dir.path().join("marker"), "the root\n").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();

    bin()
        .current_dir(elsewhere.path())
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("the root"));
}

#[test]
fn the_format_line_runs_over_the_output() {
    let dir = workspace(
        "[[status.inject]]\ntool = \"tools/many\"\ntitle = \"first only\"\nformat = \"head -1\"\n",
    );
    tool(&dir, "many", "printf 'keep\\ndrop\\n'");

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("first only:"))
        .stdout(predicate::str::contains("keep"))
        .stdout(predicate::str::contains("drop").not());
}

#[test]
fn blocks_print_in_the_order_the_manifest_declares_them() {
    let dir = workspace(
        "[[status.inject]]\ntool = \"tools/a\"\n\n\
         [[status.inject]]\ntool = \"tools/b\"\n",
    );
    tool(&dir, "a", "printf 'alpha\\n'");
    tool(&dir, "b", "printf 'beta\\n'");

    let out = bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    let alpha = out.find("alpha").expect("the first block printed");
    let beta = out.find("beta").expect("the second block printed");
    assert!(alpha < beta, "declaration order is print order:\n{out}");
}

#[test]
fn a_broken_tool_does_not_take_the_status_down_with_it() {
    // The property that makes this safe to leave in a manifest. `homma status`
    // is the cheap look you take at a workspace, and a foreign script exiting
    // non-zero is a thing to find out from it rather than a reason to refuse.
    let dir = workspace(
        "[[status.inject]]\ntool = \"tools/broken\"\n\n\
         [[status.inject]]\ntool = \"tools/fine\"\n",
    );
    tool(&dir, "broken", "printf 'no registry\\n' >&2; exit 1");
    tool(&dir, "fine", "printf 'still printed\\n'");

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace: w"))
        .stdout(predicate::str::contains("no registry"))
        .stdout(predicate::str::contains("still printed"));
}

#[test]
fn a_tool_that_is_not_there_says_so_rather_than_failing_the_command() {
    let dir = workspace("[[status.inject]]\ntool = \"tools/absent\"\n");

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("absent"))
        .stdout(predicate::str::contains("workspace: w"));
}

#[test]
fn a_workspace_declaring_none_prints_what_it_always_did() {
    // The control. Every case above would pass against a status that printed
    // something extra unconditionally, and a workspace with no injections is
    // nearly all of them.
    let dir = workspace("");

    bin()
        .args(["--dir", dir.path().to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace: w"))
        .stdout(predicate::str::contains("inject").not());
}

#[test]
fn the_json_document_carries_the_blocks_too() {
    let dir = workspace("[[status.inject]]\ntool = \"tools/context\"\ntitle = \"window\"\n");
    tool(&dir, "context", "printf '45%%\\n'");

    let out = bin()
        .args(["--dir", dir.path().to_str().unwrap(), "--output", "json", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let doc: serde_json::Value = serde_json::from_slice(&out).expect("the status is json");
    assert_eq!(doc["injected"][0]["title"], "window");
    assert_eq!(doc["injected"][0]["text"], "45%");
}
