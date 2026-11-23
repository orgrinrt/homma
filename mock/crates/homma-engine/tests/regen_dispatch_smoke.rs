//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The regen writes one dispatcher and its table, registers it once per event,
//! and a run over one member keeps the others' hooks.

use assert_cmd::Command;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

/// A member `name` under `ws` with one executable hook `guard.sh`, registered
/// at `PreToolUse` for `Bash`.
fn member(ws: &std::path::Path, name: &str) {
    let repo = ws.join(name);
    std::fs::create_dir_all(repo.join(".claude/hooks")).unwrap();
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["init", "-q"])
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git init: {out:?}");
    let hook = repo.join(".claude/hooks/guard.sh");
    std::fs::write(&hook, "#!/bin/sh\ncat > /dev/null\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(
        repo.join(".claude/settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/guard.sh","timeout":30}]}]}}"#,
    )
    .unwrap();
}

/// The `from` of every entry in the table, sorted.
fn from(ws: &std::path::Path) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.join(".claude/hooks/_dispatch.json")).unwrap(),
    )
    .unwrap();
    let mut out: Vec<String> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["from"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn a_run_over_one_member_keeps_the_others_hooks_and_a_member_gone_takes_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let ws = dir.path().join("workspace");
    member(&ws, "arvo");
    member(&ws, "kolli");
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "[workspace]\nname = \"ws\"\npath = \"{}\"\n\n[forges.github]\nkind = \"github\"\n\
             base_url = \"https://github.com\"\napi_url = \"https://api.github.com\"\n\n\
             [agent]\ntimeout = 45\n",
            ws.display()
        ),
    )
    .unwrap();
    let regen = |args: &[&str]| {
        let out = bin()
            .env("HOME", &home)
            .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
            .args(args)
            .args(["--skip-cargo-mock", "--skip-configs", "--skip-surfaces"])
            .output()
            .expect("the binary runs");
        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    };

    regen(&[]);
    assert_eq!(from(&ws), vec!["_workspace", "arvo", "kolli"]);
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(pre.len(), 1, "one registration for the event: {settings}");
    assert_eq!(
        pre[0]["hooks"][0]["command"],
        "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_dispatch.sh PreToolUse sync"
    );
    // the members' 30 and the gate's 45, the manifest's default
    assert_eq!(pre[0]["hooks"][0]["timeout"], 55);
    assert!(ws.join(".claude/hooks/_dispatch.sh").is_file());

    // A run over one member keeps the other's entry.
    regen(&["--repo", "arvo"]);
    assert_eq!(from(&ws), vec!["_workspace", "arvo", "kolli"]);

    // A member that is gone takes its entries with it.
    std::fs::remove_dir_all(ws.join("kolli")).unwrap();
    regen(&["--repo", "arvo"]);
    assert_eq!(from(&ws), vec!["_workspace", "arvo"]);

    // A settings file that does not parse fails the run and leaves the wrappers
    // an earlier regen wrote, so the hooks they run keep running until it is
    // fixed; with it fixed they go.
    let old = ws.join(".claude/hooks/arvo--guard.sh");
    std::fs::write(
        &old,
        "#!/usr/bin/env bash\n# Aggregated from `arvo` by `homma agent regen`.\n",
    )
    .unwrap();
    let local = ws.join(".claude/settings.local.json");
    let good = std::fs::read_to_string(&local).unwrap();
    std::fs::write(&local, "{ \"hooks\": {}, }").unwrap();
    let out = bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .args(["--skip-cargo-mock", "--skip-configs", "--skip-surfaces"])
        .output()
        .expect("the binary runs");
    assert!(
        !out.status.success(),
        "a settings file that does not parse passed"
    );
    assert!(
        old.exists(),
        "a wrapper went while its registration could not be swept"
    );
    std::fs::write(&local, good).unwrap();
    regen(&[]);
    assert!(!old.exists(), "the wrapper stayed once the settings moved");
}
