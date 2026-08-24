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
fn verify_says_nothing_about_a_repo_this_workspace_has_not_cloned() {
    // A workspace clones the repos its work touches. The manifest names every
    // repo there is, so most are absent from any given one, and reporting each
    // as a warning buried the findings that mean something under nineteen
    // lines of noise.
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[repos.absent]
forge = "github"
owner = "x"
local_path = "absent"
"#,
    )
    .unwrap();
    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("absent").not());
}

#[test]
fn verify_does_not_reach_a_forge_unless_asked_to() {
    // The control on the flag. The api_url is unroutable, so a lookup fails
    // loudly; the default run must not make one, and `--forge` with a token
    // must. Without both halves a `--forge` that quietly did nothing would look
    // identical to one that worked.
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[forges.nowhere]
kind = "github"
base_url = "https://127.0.0.1:9"
api_url = "https://127.0.0.1:9"
token_env = "HOMMA_TEST_NOWHERE_TOKEN"

[repos.somerepo]
forge = "nowhere"
owner = "x"
local_path = "somerepo"
"#,
    )
    .unwrap();

    // Without the flag, nothing about the forge at all.
    bin()
        .env("HOMMA_TEST_NOWHERE_TOKEN", "t")
        .args(["-c", cfg_path.to_str().unwrap(), "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nowhere").not())
        .stdout(predicate::str::contains("forge_unreachable").not());

    // With it, a connection was attempted and refused. The assertion is on the
    // finding kind rather than on the repo name: the repo is not named here,
    // because the credential check runs per forge and fails before any repo is
    // asked about, and matching a repo name would have been satisfied by three
    // different findings with three different meanings.
    bin()
        .env("HOMMA_TEST_NOWHERE_TOKEN", "t")
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("forge_unreachable"))
        .stdout(predicate::str::contains("Connection refused"));
}

#[test]
fn verify_will_not_claim_a_repo_is_absent_from_a_forge_it_cannot_authenticate_to() {
    // Every repo in this workspace is private, and GitHub answers 404 for a
    // private repo exactly as it does for one that is not there. So an
    // unauthenticated negative is not evidence, and saying otherwise made the
    // check fire on all twenty-four the first time it ran.
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[forges.nowhere]
kind = "github"
base_url = "https://127.0.0.1:9"
api_url = "https://127.0.0.1:9"
token_env = "HOMMA_TEST_NOWHERE_TOKEN"

[repos.somerepo]
forge = "nowhere"
owner = "x"
local_path = "somerepo"
"#,
    )
    .unwrap();

    bin()
        .env_remove("HOMMA_TEST_NOWHERE_TOKEN")
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        // a warning, not a failure, and nothing said about the repo itself
        .success()
        .stdout(predicate::str::contains("forge_answers_are_not_evidence"))
        .stdout(predicate::str::contains("repo_not_on_forge").not());

    // An empty token counts as no token, which is the case a bare
    // `is_some()` check would have got wrong. Asserted the same way as the
    // unset arm above, both halves: the warning appears AND nothing is said
    // about the repo. Asserting only the warning leaves the half that matters
    // uncovered on the subtler of the two cases.
    bin()
        .env("HOMMA_TEST_NOWHERE_TOKEN", "")
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("forge_answers_are_not_evidence"))
        .stdout(predicate::str::contains("repo_not_on_forge").not());
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
        .args(["-c", cfg.to_str().unwrap(), "migrate", "notko", "--to", "codeberg"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("forge `codeberg` not declared"));
}

#[test]
fn migrate_undeclared_repo_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "migrate", "missing", "--to", "github"])
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
        .args(["-c", cfg.to_str().unwrap(), "archive", "notko", "--from", "doesnotexist"])
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

#[test]
fn a_forge_whose_credential_comes_from_a_command_is_not_reported_as_tokenless() {
    // The end-to-end shape the `[auth] token_cmd` line exists for: no variable
    // is set anywhere and the credential comes from whatever tool holds it.
    // Without this, a manifest that opts in entirely through commands is told
    // to go and set an environment variable it never named.
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[auth]
token_cmd = ["printf", "a-token-for-{forge}\n"]

[forges.nowhere]
kind = "github"
base_url = "https://127.0.0.1:9"
api_url = "https://127.0.0.1:9"

[repos.somerepo]
forge = "nowhere"
owner = "x"
local_path = "somerepo"
"#,
    )
    .unwrap();

    // A credential was found, so the run gets as far as trying to use it and
    // fails on the closed port rather than on the absence of a token.
    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("forge_unreachable"))
        .stdout(predicate::str::contains("no credential").not());
}

#[test]
fn a_forge_whose_command_produces_nothing_is_reported_with_the_command_named() {
    // The control on the test above, and the diagnostic that makes the feature
    // usable: told a credential is missing, the operator is told where it was
    // looked for rather than being sent to guess.
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("homma.toml");
    std::fs::write(
        &cfg_path,
        r#"
[workspace]
name = "ws"

[auth]
token_cmd = ["false"]

[forges.nowhere]
kind = "github"
base_url = "https://127.0.0.1:9"
api_url = "https://127.0.0.1:9"

[repos.somerepo]
forge = "nowhere"
owner = "x"
local_path = "somerepo"
"#,
    )
    .unwrap();

    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no credential for forge `nowhere`"))
        .stdout(predicate::str::contains("`false` produced none"))
        // and nothing is claimed about the repo itself
        .stdout(predicate::str::contains("repo_not_on_forge").not());
}
