//! Smoke tests for the `homma` CLI.
//!
//! These cover the wiring of `clap` → command bodies → output: argument
//! parsing, config loading, JSON vs human format, and early-exit error
//! paths in the migrate / archive commands. They do not exercise live
//! network paths (`forge show` / `forge exists` / `migrate` / `archive`
//! end-to-end); the latter two would mutate real repos. Network-touching
//! smoke tests land alongside the sanity playground (#456).

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
}

fn minimal_config_toml() -> String {
    r#"
[workspace]
name = "test-ws"
path = "."

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[repos.notko]
forge = "github"
owner = "orgrinrt"
local_path = "notko"
"#
    .to_string()
}

fn write_tmp_config(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("homma.toml");
    std::fs::write(&path, minimal_config_toml()).unwrap();
    path
}

#[test]
fn version_flag_prints_version() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("homma"));
}

#[test]
fn help_flag_prints_subcommands() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("verify"))
        .stdout(predicate::str::contains("forge"))
        .stdout(predicate::str::contains("repo"))
        .stdout(predicate::str::contains("migrate"))
        .stdout(predicate::str::contains("archive"));
}

#[test]
fn status_human_renders_workspace_summary() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace: test-ws"))
        .stdout(predicate::str::contains("github"))
        .stdout(predicate::str::contains("notko"));
}

#[test]
fn status_json_renders_typed_payload() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    let out = bin()
        .args(["-c", cfg.to_str().unwrap(), "--output", "json", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = std::str::from_utf8(&out).unwrap();
    let v: serde_json::Value = serde_json::from_str(s).expect("output is valid JSON");
    assert_eq!(v["workspace"]["name"], "test-ws");
    assert_eq!(v["forges"][0]["name"], "github");
    assert_eq!(v["repos"][0]["name"], "notko");
}

#[test]
fn verify_ok_on_workspace_with_no_local_paths() {
    // local_path = "notko" does not exist, but that is a warn-level finding
    // (the directory will be created by `homma sync`). verify still exits 0.
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "verify"])
        .assert()
        .success();
}

#[test]
fn verify_fails_on_undeclared_forge() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[repos.broken]
forge = "doesnotexist"
owner = "x"
local_path = "broken"
"#,
    )
    .unwrap();
    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("repo_forge_undeclared"));
}

#[test]
fn missing_config_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("missing.toml");
    bin()
        .args(["-c", cfg.to_str().unwrap(), "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("loading config from"));
}

#[test]
fn migrate_undeclared_destination_forge_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "migrate",
            "notko",
            "--to",
            "codeberg",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("forge `codeberg` not declared"));
}

#[test]
fn migrate_undeclared_repo_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "migrate",
            "missing",
            "--to",
            "github",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repo `missing` not declared"));
}

#[test]
fn archive_undeclared_repo_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "archive", "missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("repo `missing` not declared"));
}

#[test]
fn archive_undeclared_forge_override_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args([
            "-c",
            cfg.to_str().unwrap(),
            "archive",
            "notko",
            "--from",
            "doesnotexist",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "forge `doesnotexist` not declared",
        ));
}

#[test]
fn a_hand_edited_handle_carrying_a_parent_component_is_refused_on_read() {
    // `check_handle` ran on `org add` only, and the registry is hand-editable
    // on purpose, so a handle arriving any other way reached `Layout::home`
    // unchecked and addressed a tree outside the workspace.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        "content_repo = \"local\"\n\n[org.\"../escape\"]\nrole = \"hand\"\nhandle = \"../escape\"\n",
    )
    .unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path separator"));
}

#[test]
fn a_table_key_disagreeing_with_its_handle_is_refused() {
    // Two names for one participant, one of which everything addresses and the
    // other of which is silently ignored.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        "content_repo = \"local\"\n\n[org.paja]\nrole = \"hand\"\nhandle = \"someone-else\"\n",
    )
    .unwrap();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must agree"));
}
