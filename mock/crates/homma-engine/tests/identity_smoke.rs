//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma identity`: a character file becomes the definition a session loads,
//! and a launch starts the session as that agent.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use predicates::prelude::*;

const COORD: &str = "---\nname: coord\nrole: the kaski coordinator\nmemories: [.shared/hands/coord/memory]\n---\n\n# coord\n\n## Personality\n\nPlain and short.\n";

fn workspace() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    fs::write(&cfg, support::minimal_config_toml()).unwrap();
    let hands = dir.path().join(".shared/hands/coord");
    fs::create_dir_all(&hands).unwrap();
    fs::write(hands.join("character.md"), COORD).unwrap();
    (dir, cfg)
}

fn run(cfg: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    support::bin()
        .args(["--config", cfg.to_str().unwrap(), "identity"])
        .args(args)
        .assert()
}

#[test]
fn render_writes_the_definition_and_links_the_memory() {
    let (dir, cfg) = workspace();
    run(&cfg, &["render"])
        .success()
        .stdout(predicate::str::contains("coord"));
    let def = fs::read_to_string(dir.path().join(".claude/agents/coord.md")).unwrap();
    assert!(
        def.contains("name: coord\n") && def.contains("Plain and short."),
        "{def}"
    );
    assert_eq!(
        fs::canonicalize(dir.path().join(".claude/agent-memory/coord")).unwrap(),
        fs::canonicalize(dir.path().join(".shared/hands/coord/memory")).unwrap()
    );
}

#[test]
fn render_refuses_a_memory_that_leaves_shared_and_writes_nothing() {
    let (dir, cfg) = workspace();
    let bad = COORD.replace(".shared/hands/coord/memory", "../outside");
    fs::write(dir.path().join(".shared/hands/coord/character.md"), bad).unwrap();
    run(&cfg, &["render", "coord"])
        .failure()
        .stderr(predicate::str::contains("memory"));
    assert!(!dir.path().join(".claude/agents/coord.md").exists());
}

#[test]
fn launch_starts_the_session_as_the_agent_with_its_identity_in_the_environment() {
    let (dir, cfg) = workspace();
    let out = dir.path().join("seen");
    let stub = dir.path().join("claude-stub");
    fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho \"$HOMMA_IDENTITY|$*\" > '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

    support::bin()
        .env("HOMMA_CLAUDE", &stub)
        .args(["--config", cfg.to_str().unwrap(), "identity", "launch", "coord", "--resume"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(out).unwrap(),
        "coord|--agent coord --resume\n"
    );
    assert!(
        dir.path().join(".claude/agents/coord.md").exists(),
        "the definition is rendered before the session starts"
    );
}

#[test]
fn launch_as_an_unknown_identity_starts_nothing() {
    let (dir, cfg) = workspace();
    let out = dir.path().join("seen");
    let stub = dir.path().join("claude-stub");
    fs::write(
        &stub,
        format!("#!/bin/sh\necho ran > '{}'\n", out.display()),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    support::bin()
        .env("HOMMA_CLAUDE", &stub)
        .args(["--config", cfg.to_str().unwrap(), "identity", "launch", "nobody"])
        .assert()
        .failure();
    assert!(!out.exists());
}
