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

/// A program standing in for `claude` that writes what it was given to `out`.
fn stub(dir: &Path, script: &str) -> std::path::PathBuf {
    let stub = dir.join("claude-stub");
    fs::write(&stub, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    stub
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
        .stderr(predicate::str::contains(
            "a relative path to a directory below `.shared`",
        ));
    assert!(!dir.path().join(".claude/agents/coord.md").exists());
}

#[test]
fn render_refuses_shared_itself_as_a_memory() {
    let (dir, cfg) = workspace();
    let bad = COORD.replace(".shared/hands/coord/memory", ".shared");
    fs::write(dir.path().join(".shared/hands/coord/character.md"), bad).unwrap();
    run(&cfg, &["render", "coord"])
        .failure()
        .stderr(predicate::str::contains("below `.shared`"));
    assert!(!dir.path().join(".claude/agent-memory/coord").exists());
}

#[test]
fn render_refuses_two_identities_that_keep_one_memory_and_writes_neither() {
    let (dir, cfg) = workspace();
    let twin = dir.path().join(".shared/hands/second");
    fs::create_dir_all(&twin).unwrap();
    // A memory outside the hands directory, which nobody keeps by default.
    let common = COORD.replace(".shared/hands/coord/memory", ".shared/common-mem");
    fs::write(dir.path().join(".shared/hands/coord/character.md"), &common).unwrap();
    fs::write(
        twin.join("character.md"),
        common
            .replace("name: coord", "name: second")
            .replace("# coord", "# second"),
    )
    .unwrap();
    run(&cfg, &["render"])
        .failure()
        .stderr(predicate::str::contains("a memory has one owner"));
    assert!(!dir.path().join(".claude/agents/coord.md").exists());
    assert!(!dir.path().join(".claude/agents/second.md").exists());
}

#[test]
fn render_does_not_write_through_a_link_that_leaves_the_root() {
    let (dir, cfg) = workspace();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(outside.path().join("deep/.claude")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("deep/.claude"),
        dir.path().join(".claude"),
    )
    .unwrap();
    run(&cfg, &["render"]).failure();
    assert!(
        !outside.path().join("deep/.claude/agents/coord.md").exists(),
        "nothing is written outside the workspace"
    );
}

#[test]
fn render_with_no_hands_directory_says_there_are_no_identities() {
    let (dir, cfg) = workspace();
    fs::remove_dir_all(dir.path().join(".shared/hands")).unwrap();
    run(&cfg, &["render"])
        .success()
        .stdout(predicate::str::contains("no identities are authored here"));
}

#[test]
fn render_reports_a_character_file_it_cannot_read() {
    let (dir, cfg) = workspace();
    let broken = dir.path().join(".shared/hands/broken");
    fs::create_dir_all(broken.join("character.md")).unwrap();
    run(&cfg, &["render"])
        .failure()
        .stderr(predicate::str::contains("broken/character.md"));
    assert!(
        !dir.path().join(".claude/agents/coord.md").exists(),
        "one that cannot be read stops the pass before anything is written"
    );
}

#[test]
fn a_regular_file_where_the_memory_link_goes_is_refused_by_what_it_is() {
    let (dir, cfg) = workspace();
    fs::create_dir_all(dir.path().join(".claude/agent-memory")).unwrap();
    fs::write(dir.path().join(".claude/agent-memory/coord"), "kept").unwrap();
    run(&cfg, &["render"])
        .failure()
        .stderr(predicate::str::contains("is not a link"));
    assert_eq!(
        fs::read_to_string(dir.path().join(".claude/agent-memory/coord")).unwrap(),
        "kept"
    );
}

#[test]
fn a_persona_of_an_identitys_name_is_refused() {
    let (dir, cfg) = workspace();
    let personas = dir.path().join(".shared/agents");
    fs::create_dir_all(&personas).unwrap();
    fs::write(
        personas.join("coord.md.tmpl"),
        "---\nname: coord\ndescription: A persona.\n---\n\nBody.\n",
    )
    .unwrap();
    run(&cfg, &["render"])
        .failure()
        .stderr(predicate::str::contains(
            "both an authored persona and an identity",
        ));
    support::bin()
        .args(["--config", cfg.to_str().unwrap(), "agents", "render"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "both an authored persona and an identity",
        ));
}

#[test]
fn an_identitys_definition_is_not_reported_as_stray_by_the_agents_pass() {
    let (dir, cfg) = workspace();
    run(&cfg, &["render"]).success();
    let personas = dir.path().join(".shared/agents");
    fs::create_dir_all(&personas).unwrap();
    fs::write(
        personas.join("reviewer.md.tmpl"),
        "---\nname: reviewer\ndescription: A reviewer.\n---\n\nBody.\n",
    )
    .unwrap();
    support::bin()
        .args(["--config", cfg.to_str().unwrap(), "agents", "render"])
        .assert()
        .success()
        .stdout(predicate::str::contains("authored nowhere").not());
}

#[test]
fn an_unrelated_broken_character_does_not_stop_the_agents_pass() {
    let (dir, cfg) = workspace();
    // Frontmatter that never closes: broken, and not the personas' business.
    let broken = dir.path().join(".shared/hands/broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("character.md"), "---\nname: broken\nrole: x\n").unwrap();
    let personas = dir.path().join(".shared/agents");
    fs::create_dir_all(&personas).unwrap();
    fs::write(
        personas.join("reviewer.md.tmpl"),
        "---\nname: reviewer\ndescription: A reviewer.\n---\n\nBody.\n",
    )
    .unwrap();
    support::bin()
        .args(["--config", cfg.to_str().unwrap(), "agents", "render"])
        .assert()
        .success();
    assert!(dir.path().join(".claude/agents/reviewer.md").exists());
    // And the command about identities still says what is wrong.
    run(&cfg, &["render"])
        .failure()
        .stderr(predicate::str::contains("broken/character.md"));
}

#[test]
fn a_persona_of_a_broken_identitys_name_is_still_refused() {
    let (dir, cfg) = workspace();
    let broken = dir.path().join(".shared/hands/reviewer");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("character.md"), "---\nname: reviewer\n").unwrap();
    let personas = dir.path().join(".shared/agents");
    fs::create_dir_all(&personas).unwrap();
    fs::write(
        personas.join("reviewer.md.tmpl"),
        "---\nname: reviewer\ndescription: A reviewer.\n---\n\nBody.\n",
    )
    .unwrap();
    support::bin()
        .args(["--config", cfg.to_str().unwrap(), "agents", "render"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "both an authored persona and an identity",
        ));
}

#[test]
fn launch_starts_the_session_as_the_agent_in_the_workspace() {
    let (dir, cfg) = workspace();
    let out = dir.path().join("seen");
    let stub = stub(
        dir.path(),
        &format!("echo \"$*|$PWD\" > '{}'", out.display()),
    );

    support::bin()
        .env("HOMMA_CLAUDE", &stub)
        .args(["--config", cfg.to_str().unwrap(), "identity", "launch", "coord", "--resume"])
        .assert()
        .success();
    let seen = fs::read_to_string(out).unwrap();
    let (args, cwd) = seen.trim_end().split_once('|').unwrap();
    assert_eq!(args, "--agent coord --resume");
    assert_eq!(
        fs::canonicalize(cwd).unwrap(),
        fs::canonicalize(dir.path()).unwrap()
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
    let stub = stub(dir.path(), &format!("echo ran > '{}'", out.display()));
    support::bin()
        .env("HOMMA_CLAUDE", &stub)
        .args(["--config", cfg.to_str().unwrap(), "identity", "launch", "nobody"])
        .assert()
        .failure();
    assert!(!out.exists());
}

#[test]
fn launch_under_a_configured_agents_place_refuses_and_starts_nothing() {
    let (dir, cfg) = workspace();
    let mut text = fs::read_to_string(&cfg).unwrap();
    text.push_str("\n[paths]\nagents = \"elsewhere/agents\"\n");
    fs::write(&cfg, text).unwrap();
    let out = dir.path().join("seen");
    let stub = stub(dir.path(), &format!("echo ran > '{}'", out.display()));
    support::bin()
        .env("HOMMA_CLAUDE", &stub)
        .args(["--config", cfg.to_str().unwrap(), "identity", "launch", "coord"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("elsewhere/agents"));
    assert!(!out.exists(), "no session was started");
    assert!(
        !dir.path().join("elsewhere/agents/coord.md").exists(),
        "and nothing was rendered for it"
    );
}
