//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke tests for the `homma` CLI.
//!
//! These cover the wiring of `clap` → command bodies → output: argument
//! parsing, config loading, JSON vs human format, and early-exit error
//! paths in the migrate / archive commands. They do not exercise live
//! network paths (`forge show` / `forge exists` / `migrate` / `archive`
//! end-to-end); the latter two would mutate real repos. Network-touching
//! smoke tests land alongside the sanity playground (#456).
//!
//! Every fixture here plants real clones beside the manifest. Membership is
//! read off the tree, so a test about a member needs one on disk; a table of
//! names in the manifest is not a thing that parses any more.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
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
"#
    .to_string()
}

/// A manifest beside one clone, which is the smallest workspace that has a
/// member at all.
fn write_tmp_config(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("homma.toml");
    std::fs::write(&path, minimal_config_toml()).unwrap();
    clone_at(
        dir.path(),
        "notko",
        Some("https://github.com/orgrinrt/notko.git"),
    );
    path
}

/// A real repository at `root/name`, with `origin` set to `url` when one is
/// given.
///
/// A real `git init` rather than a hand-made `.git` directory, because the
/// origin is read through git and a fake would answer nothing.
fn clone_at(root: &Path, name: &str, url: Option<&str>) {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    run(&["init", "-q"]);
    if let Some(url) = url {
        run(&["remote", "add", "origin", url]);
    }
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
fn a_member_s_branches_are_the_workspace_defaults_because_there_is_nowhere_else() {
    // A per-repository override used to live on the declared row. Detection
    // has no row to write one in, so the workspace default is the whole
    // answer, and a member line carrying anything else would mean an override
    // came back without a place to be set.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("homma.toml");
    std::fs::write(
        &path,
        format!(
            "{}\n[defaults]\npublic_branch = \"trunk\"\nworking_branch = \"next\"\n",
            minimal_config_toml()
        ),
    )
    .unwrap();
    clone_at(
        dir.path(),
        "notko",
        Some("https://github.com/orgrinrt/notko.git"),
    );
    let out = bin()
        .args(["-c", path.to_str().unwrap(), "--output", "json", "status"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out).unwrap()).expect("output is valid JSON");
    assert_eq!(v["workspace"]["default_public_branch"], "trunk");
    assert_eq!(v["workspace"]["default_working_branch"], "next");
    assert_eq!(v["repos"][0]["public_branch"], "trunk");
    assert_eq!(v["repos"][0]["working_branch"], "next");
}

#[test]
fn verify_is_quiet_on_a_workspace_whose_clones_all_sit_on_a_declared_forge() {
    // The resting state, and the control on every finding below it: a member
    // whose origin names a forge this manifest has a profile for is nothing to
    // report.
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "verify"])
        .assert()
        .success()
        // The whole output, not a substring of it. A `.success()` alone passes
        // for a run that reports a page of warnings, which is exactly the state
        // this is the control for.
        .stdout(predicate::function(|out: &[u8]| {
            std::str::from_utf8(out).unwrap().trim() == "OK"
        }));
}

#[test]
fn verify_names_a_clone_whose_remote_sits_on_no_forge_this_workspace_knows() {
    // A warning rather than a failure, because the clone is real and the work
    // in it is fine. What it costs is that every forge operation against it
    // needs the forge passed by hand, and the operator should hear that once
    // rather than find out at the first push.
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
"#,
    )
    .unwrap();
    clone_at(
        dir.path(),
        "broken",
        Some("https://git.example.invalid/x/broken.git"),
    );
    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("repo_forge_unknown"))
        .stdout(predicate::str::contains("broken"));
}

#[test]
fn verify_says_nothing_about_a_directory_that_is_not_a_repository() {
    // A workspace root holds scratch directories, build output and notes. None
    // of them is a member, and the whole of the difference is a `.git`, so this
    // is the end-to-end check that the tree is being read rather than listed.
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
"#,
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("scratch")).unwrap();
    clone_at(
        dir.path(),
        "real",
        Some("https://github.com/orgrinrt/real.git"),
    );
    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify"])
        .assert()
        .success()
        .stdout(predicate::str::contains("scratch").not());
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
"#,
    )
    .unwrap();
    clone_at(
        dir.path(),
        "somerepo",
        Some("https://127.0.0.1:9/x/somerepo.git"),
    );

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
"#,
    )
    .unwrap();
    clone_at(
        dir.path(),
        "somerepo",
        Some("https://127.0.0.1:9/x/somerepo.git"),
    );

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
fn migrate_a_repo_the_workspace_does_not_hold_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "migrate", "missing", "--to", "github"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no repository named `missing` under the workspace root",
        ));
}

#[test]
fn archive_a_repo_the_workspace_does_not_hold_errors_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = write_tmp_config(&dir);
    bin()
        .args(["-c", cfg.to_str().unwrap(), "archive", "missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no repository named `missing` under the workspace root",
        ));
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
"#,
    )
    .unwrap();
    clone_at(
        dir.path(),
        "somerepo",
        Some("https://127.0.0.1:9/x/somerepo.git"),
    );

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
"#,
    )
    .unwrap();
    clone_at(
        dir.path(),
        "somerepo",
        Some("https://127.0.0.1:9/x/somerepo.git"),
    );

    bin()
        .args(["-c", cfg_path.to_str().unwrap(), "verify", "--forge"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no credential for forge `nowhere`"))
        .stdout(predicate::str::contains("`false` produced none"))
        // and nothing is claimed about the repo itself
        .stdout(predicate::str::contains("repo_not_on_forge").not());
}

/// A member with one committed crate, so the release subcommands have a
/// manifest and a history to read.
fn committed_crate(root: &Path, name: &str) {
    clone_at(root, name, Some("https://github.com/orgrinrt/x.git"));
    let path = root.join(name);
    std::fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for args in [
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "."],
        vec!["commit", "-qm", "feat: first"],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(&args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
}

#[test]
fn release_is_listed_and_plan_prints_the_next_version_without_moving_anything() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    bin()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("release"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "minor"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "version 0.1.0 becomes 0.2.0, tagged `v0.2.0`",
        ))
        .stdout(predicate::str::contains("feat: first"))
        .stdout(predicate::str::contains("x to crates-io"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--level"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "plan", "x", "--level", "huge"])
        .assert()
        .failure();
    assert!(!dir.path().join("x/CHANGELOG.md").exists());
}

#[test]
fn release_hook_install_writes_the_pre_push_and_check_reports_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "hook", "install", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hooks/pre-push"));
    let script = std::fs::read_to_string(dir.path().join("x/.git/hooks/pre-push")).unwrap();
    assert!(script.contains("homma release gate --hook"));
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "hook", "install", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("`nope` is not a repository"));
}

#[test]
fn release_badges_reads_the_newest_run_whichever_branch_it_measured() {
    use homma_api::{GateRun, Step, StepOutcome, Verdict};
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, minimal_config_toml()).unwrap();
    committed_crate(dir.path(), "x");
    let repo = dir.path().join("x");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    // a `dev`/`main` repo whose trunk is one commit past the release line,
    // pushing to a bare origin so the badges branch has somewhere to land
    let bare = dir.path().join("origin.git");
    std::process::Command::new("git")
        .args(["init", "-q", "--bare", bare.to_str().unwrap()])
        .status()
        .unwrap();
    git(&["remote", "set-url", "origin", bare.to_str().unwrap()]);
    git(&["branch", "-M", "main"]);
    git(&["switch", "-qc", "dev"]);
    std::fs::write(repo.join("src.rs"), "fn f() {}\n").unwrap();
    git(&["add", "src.rs"]);
    git(&["commit", "-qm", "feat: more"]);
    let dev_tip = git(&["rev-parse", "dev"]).trim().to_string();
    // with nothing recorded the command says so and writes nothing
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "badges", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no gate run recorded for `x`"));
    // a green run on the trunk's tip, which is where the hook records them
    let store = homma_store::Store::open(dir.path().join(".data/homma"));
    let mut step = StepOutcome::skipped(Step::Tests);
    step.skipped = false;
    step.passed = true;
    let run = GateRun {
        repo:    "x".into(),
        sha:     dev_tip.clone(),
        ran_at:  "2026-09-02T22:00:00Z".into(),
        verdict: Verdict::Green,
        steps:   vec![step],
    };
    store.append(&GateRun::kind(), &run.to_record()).unwrap();
    bin()
        .args(["-c", cfg.to_str().unwrap(), "release", "badges", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote"))
        .stdout(predicate::str::contains(format!(
            "from the run on {}",
            &dev_tip[.. 7]
        )));
    let on_origin = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "branch", "--list", "badges"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&on_origin.stdout).contains("badges"));
    let gate = std::process::Command::new("git")
        .args(["-C", bare.to_str().unwrap(), "show", "badges:gate.json"])
        .output()
        .unwrap();
    assert!(gate.status.success(), "the badges branch carries gate.json");
}
