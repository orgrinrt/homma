//! `org add` writes at a path the operator names, so it is checked like one.
//!
//! It had no deny check at all, and the omission was invisible because the
//! README and the PR body both enumerated the writers with no enforcement and
//! named only the gen pass. An incomplete list of known gaps reads as a complete
//! one, which is worse than no list.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
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
