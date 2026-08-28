//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `org add` writes at a path the operator names, so it is checked like one.
//!
//! It had no deny check at all, and the omission was invisible because the
//! README and the PR body both enumerated the writers with no enforcement and
//! named only the gen pass. An incomplete list of known gaps reads as a complete
//! one, which is worse than no list.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

#[test]
fn a_registry_inside_the_operators_claude_is_refused() {
    // Reproduced at exit 0 before this: `org add intruder` printed
    // `intruder staffed` and rewrote a registry inside `~/.claude`, beside the
    // credentials the record names.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let cfg = home.join(".claude").join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "intruder"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "i", "--git-email", "i@example.invalid"])
        .args(["--workspace", dir.path().join("ws").to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        !after.contains("intruder"),
        "the registry must be exactly as it was: {after}"
    );
    assert!(
        !cfg.with_extension("toml.writing").exists(),
        "and no temporary may be left behind, since the refusal is before the write"
    );
}

#[test]
fn a_registry_somewhere_ordinary_is_still_written() {
    // The other side, because a guard that refuses everything is not a guard.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "ordinary"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "o", "--git-email", "o@example.invalid"])
        .args(["--workspace", dir.path().join("ws").to_str().unwrap()])
        .assert()
        .success();

    assert!(std::fs::read_to_string(&cfg).unwrap().contains("ordinary"));
}

#[test]
fn a_registry_inside_another_participants_workspace_is_refused() {
    // Deny item two on the `org add` path, which carried only the home-derived
    // pair. Reproduced at exit 0: a registry rewritten inside a workspace the
    // registry itself declares as somebody's.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();

    let theirs = dir.path().join("ws-a");
    std::fs::create_dir_all(&theirs).unwrap();
    let cfg = theirs.join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "content_repo = \"local\"\n\n[org.a]\nrole = \"hand\"\nstaffed = true\nhandle = \"a\"\ngit_name = \"a\"\ngit_email = \"a@example.invalid\"\nworkspace = \"{}\"\n",
            theirs.display()
        ),
    )
    .unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "intruder"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "i", "--git-email", "i@example.invalid"])
        .args(["--workspace", dir.path().join("ws").to_str().unwrap()])
        .assert()
        .failure()
        // "a participant's" rather than "another participant's": the registry
        // belongs to the workspace rather than to anybody in it, so nobody is
        // excluded from this list and there is no "other" to be relative to.
        .stderr(predicate::str::contains("a participant's workspace"));

    assert!(!std::fs::read_to_string(&cfg).unwrap().contains("intruder"));
}

#[test]
fn a_registry_in_a_place_the_manifest_denies_is_refused() {
    // The manifest's own list, through `org add` rather than through
    // `org up`. Both fold the list in and only one of them was asserted, so
    // deleting the fold from the constructor this command uses left the whole
    // suite green.
    //
    // The registry sits somewhere ordinary and the `deny` entry names where
    // the write would land, which is what separates this from the `~/.claude`
    // case above: nothing about the destination is special except that the
    // operator wrote it down.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let theirs = home.join("work").join("someone-elses");
    std::fs::create_dir_all(&theirs).unwrap();
    let cfg = theirs.join("homma.toml");
    std::fs::write(
        &cfg,
        "content_repo = \"local\"\ndeny = [{ path = \"~/work/someone-elses\", \
         why = \"it belongs to somebody else\" }]\n",
    )
    .unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "intruder"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "i", "--git-email", "i@example.invalid"])
        .args(["--workspace", dir.path().join("ws").to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("it belongs to somebody else"));

    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        !after.contains("intruder"),
        "the registry must be exactly as it was: {after}"
    );
}

#[test]
fn a_registry_the_manifest_does_not_deny_is_written() {
    // The control, and the reason the test above is worth having: a build that
    // refused every registry write would satisfy it just as well. Same tree,
    // same command, the `deny` key removed.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let theirs = home.join("work").join("someone-elses");
    std::fs::create_dir_all(&theirs).unwrap();
    let cfg = theirs.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "resident"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "r", "--git-email", "r@example.invalid"])
        .args(["--workspace", dir.path().join("ws").to_str().unwrap()])
        .assert()
        .success();

    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        after.contains("resident"),
        "the entry should have landed: {after}"
    );
}
