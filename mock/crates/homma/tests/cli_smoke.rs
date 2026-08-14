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

/// A registry with a comment and one entry, to add to.
fn registry_with_a_comment(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("homma.toml");
    std::fs::write(
        &path,
        "# a comment that must survive\ncontent_repo = \"clause-dev\"\n\n\
         [org.op]\nrole = \"king\"\nhandle = \"op\"\n",
    )
    .unwrap();
    path
}

#[test]
fn adding_an_entry_appends_and_leaves_the_rest_of_the_file_alone() {
    // Serialising the whole registry back would round-trip away the comments
    // and the ordering somebody chose, silently, and the file is hand-edited.
    let dir = tempfile::tempdir().unwrap();
    let path = registry_with_a_comment(&dir);

    bin()
        .args([
            "--config",
            path.to_str().unwrap(),
            "org",
            "add",
            "rendering",
        ])
        .args(["--role", "hand", "--domain", "rendering"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mapped"));

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# a comment that must survive"),
        "the comment must survive being added to:\n{text}"
    );
    assert!(text.contains("[org.rendering]"));
    assert!(text.contains("staffed = false"));
}

#[test]
fn an_added_entry_parses_back_and_reports_as_mapped() {
    // The round trip is the point: an entry homma writes and cannot read is
    // worse than one it refuses to write.
    let dir = tempfile::tempdir().unwrap();
    let path = registry_with_a_comment(&dir);
    bin()
        .args([
            "--config",
            path.to_str().unwrap(),
            "org",
            "add",
            "rendering",
        ])
        .args(["--role", "hand", "--domain", "rendering"])
        .assert()
        .success();

    bin()
        .args(["--config", path.to_str().unwrap(), "org", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rendering"))
        .stdout(predicate::str::contains("mapped"));
}

#[test]
fn standing_up_a_mapped_entry_is_refused_and_says_it_is_mapped() {
    // The message is the test. Reporting three absent fields would be true and
    // would send somebody off to fill them in when the entry is finished.
    let dir = tempfile::tempdir().unwrap();
    let path = registry_with_a_comment(&dir);
    bin()
        .args([
            "--config",
            path.to_str().unwrap(),
            "org",
            "add",
            "rendering",
        ])
        .args(["--role", "hand"])
        .assert()
        .success();

    bin()
        .args(["--config", path.to_str().unwrap(), "org", "up", "rendering"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mapped, not staffed"));
}

#[test]
fn adding_a_handle_that_would_escape_its_directory_is_refused_at_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let path = registry_with_a_comment(&dir);
    bin()
        .args(["--config", path.to_str().unwrap(), "org", "add", "../evil"])
        .args(["--role", "hand"])
        .assert()
        .failure();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("evil"),
        "a refused entry must not reach the file:\n{text}"
    );
}

/// A content repository with one commit, and a clone of it to run homma from.
///
/// Built with the git binary because it is scaffolding, not the thing tested.
fn content_repo_and_root(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let run = |args: &[&str], at: &std::path::Path| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(at)
            .status()
            .expect("git should run")
            .success();
        assert!(ok, "git {args:?} failed");
    };
    let src = dir.join("content");
    std::fs::create_dir_all(&src).unwrap();
    run(&["init", "-q", "-b", "main"], &src);
    run(&["config", "user.name", "src"], &src);
    run(&["config", "user.email", "src@example.invalid"], &src);
    std::fs::write(src.join("README.md"), "content").unwrap();
    run(&["add", "README.md"], &src);
    run(&["commit", "-q", "-m", "initial", "--no-gpg-sign"], &src);

    let root = dir.join("root");
    run(
        &["clone", "-q", src.to_str().unwrap(), root.to_str().unwrap()],
        dir,
    );
    std::fs::write(root.join("homma.toml"), "content_repo = \"content\"\n").unwrap();
    (src, root)
}

/// The exit test for standing an identity up, end to end against a real
/// repository.
///
/// This is the test whose absence let a round ship where `provision` had no
/// caller: `org up` exited 0, created directories, cloned nothing, set no
/// identity, and generated a definition telling the Hand it commits as an
/// identity nothing had configured. Every unit test passed, because each tested
/// a function nothing called.
#[test]
fn standing_up_clones_the_workspace_and_sets_the_identity_in_that_clone() {
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    let ws = dir.path().join("ws").join("fresh");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "fresh"])
        .args(["--role", "hand", "--staffed"])
        .args([
            "--git-name",
            "fresh",
            "--git-email",
            "fresh@example.invalid",
        ])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "fresh"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("cloned"));

    assert!(
        ws.join(".git").exists(),
        "the workspace must actually be a clone"
    );

    // The identity in that clone's own config, which is the requirement.
    let local = std::fs::read_to_string(ws.join(".git/config")).unwrap();
    assert!(
        local.contains("fresh@example.invalid"),
        "the identity must be in the clone's own config:\n{local}"
    );
}

#[test]
fn standing_up_twice_reports_the_workspace_was_already_there() {
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    let ws = dir.path().join("ws").join("fresh");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "fresh"])
        .args(["--role", "hand", "--staffed"])
        .args([
            "--git-name",
            "fresh",
            "--git-email",
            "fresh@example.invalid",
        ])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    for _ in 0..2 {
        bin()
            .args(["--config", cfg.to_str().unwrap(), "org", "up", "fresh"])
            .current_dir(&root)
            .assert()
            .success();
    }

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "fresh"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicate::str::contains("already there"));
}

#[test]
fn standing_up_from_a_root_that_is_not_a_clone_is_refused() {
    // The clone URL comes from the root's own origin. A root with none has
    // nothing to derive it from, and guessing would clone the wrong thing.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bare");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"content\"\n").unwrap();
    let ws = dir.path().join("ws").join("fresh");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "fresh"])
        .args(["--role", "hand", "--staffed"])
        .args([
            "--git-name",
            "fresh",
            "--git-email",
            "fresh@example.invalid",
        ])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "fresh"])
        .current_dir(&root)
        .assert()
        .failure();

    assert!(!ws.exists(), "a refusal must leave nothing behind");
}
