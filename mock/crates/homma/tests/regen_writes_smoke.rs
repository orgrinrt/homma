//! The aggregation pass aggregates into a directory, and it may not be a home's.
//!
//! **Reproduced at exit 0 before this.** With `workspace.path = $HOME`, `agent
//! regen` wrote two mode-`0755` scripts into `$HOME/.claude/hooks/` and rewrote
//! `$HOME/.claude/settings.json` to register them as `PreToolUse` hooks. That is
//! the record's third denied location verbatim, and it installs code the harness
//! then executes, beside the credentials that live there.
//!
//! **What is deliberately not refused is a workspace that is the central clone.**
//! Aggregating into a checkout is this pass's whole purpose, and the workspace
//! configuration that ships in clause-dev names the central clone as its own
//! path. Refusing it forbids the tool its purpose, and answering that needs a
//! decision about who homma is acting as. A previous round bundled the two
//! questions and deferred both under an argument covering only the first.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
}

fn config_at(workspace: &std::path::Path) -> String {
    format!(
        r#"
[workspace]
name = "ws"
path = "{}"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
"#,
        workspace.display()
    )
}

#[test]
fn aggregating_into_the_operators_own_claude_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude").join("settings.json"), "{}").unwrap();

    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, config_at(&home)).unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !home.join(".claude").join("hooks").exists(),
        "no hook script may land beside the operator's own settings"
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".claude").join("settings.json")).unwrap(),
        "{}",
        "and the settings file must be exactly as it was"
    );
}

#[test]
fn aggregating_into_an_ordinary_workspace_is_not_refused() {
    // The other side, because a guard that refuses everything is not a guard.
    // It gets past the deny check and then fails on its own terms, having no
    // repositories to walk, which is what distinguishes the two: the refusal
    // above names the record, this one does not.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();

    let ws = dir.path().join("workspace");
    std::fs::create_dir_all(&ws).unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, config_at(&ws)).unwrap();

    let out = bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("nothing may be written there"),
        "an ordinary workspace must not be refused by the deny list: {stderr}"
    );
}
