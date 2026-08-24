//! The aggregation pass aggregates into a directory, and it may not be a home's.
//!
//! **Reproduced at exit 0 before this.** With `workspace.path = $HOME`, `agent
//! regen` wrote two mode-`0755` scripts into `$HOME/.claude/hooks/` and rewrote
//! `$HOME/.claude/settings.json` to register them as `PreToolUse` hooks. That is
//! the record's third denied location verbatim, and it installs code the harness
//! then executes, beside the credentials that live there.
//!
//! **What is deliberately not refused is a workspace that is somebody's own.**
//! Aggregating into a checkout is this pass's whole purpose, and the workspace
//! configuration that ships in the workspace repo names that workspace as its own
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

#[test]
fn a_symlink_below_the_checked_directory_cannot_carry_a_write_out() {
    // **The reproduction the previous round's check could not see.** It guarded
    // `<workspace>/.claude` as one string, and every path below it was built
    // with `Path::join`, which resolves nothing. Linking the hooks and rules
    // directories out of the workspace carried every write into the operator's
    // own `.claude`, at exit 0 printing `regen: ok`: it deleted files that were
    // there and installed two `-rwxr-xr-x` scripts in their place.
    //
    // Eighth relocation of this branch's defect, and the same shape each time: a
    // guard on a path a caller passed in, while the paths actually written to
    // were computed downstream and never guarded. What has to be proven is the
    // path `std::fs` receives.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude").join("hooks")).unwrap();
    std::fs::write(
        home.join(".claude").join("hooks").join("precious.sh"),
        "keep",
    )
    .unwrap();

    let ws = dir.path().join("workspace");
    std::fs::create_dir_all(ws.join(".claude")).unwrap();
    std::os::unix::fs::symlink(
        home.join(".claude").join("hooks"),
        ws.join(".claude").join("hooks"),
    )
    .unwrap();

    // A repository with a hook to aggregate, so the pass has work to do.
    let repo = dir.path().join("notko");
    std::fs::create_dir_all(repo.join(".claude").join("hooks")).unwrap();
    std::fs::write(
        repo.join(".claude").join("hooks").join("h.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();

    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            r#"
[workspace]
name = "ws"
path = "{}"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[repos.notko]
forge = "github"
owner = "orgrinrt"
local_path = "{}"
"#,
            ws.display(),
            repo.display()
        ),
    )
    .unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .args(["--skip-cargo-mock"])
        .assert()
        .failure();

    assert!(
        home.join(".claude")
            .join("hooks")
            .join("precious.sh")
            .exists(),
        "a file in the operator's own hooks directory must not be deleted"
    );
    let leaked: Vec<_> = std::fs::read_dir(home.join(".claude").join("hooks"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n != "precious.sh")
        .collect();
    assert!(
        leaked.is_empty(),
        "nothing may be written into the operator's own hooks directory: {leaked:?}"
    );
}

#[test]
fn a_workspace_that_is_another_participants_is_refused() {
    // Deny item two, which `agent regen` did not carry at all: it used the
    // home-derived list alone, which is two of the record's three locations,
    // while the README said it was checked against the same list as everything
    // else.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();

    let theirs = dir.path().join("ws-a");
    std::fs::create_dir_all(&theirs).unwrap();

    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            r#"
[workspace]
name = "ws"
path = "{}"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[org.a]
role = "hand"
staffed = true
handle = "a"
git_name = "a"
git_email = "a@example.invalid"
workspace = "{}"
"#,
            theirs.join("inner").display(),
            theirs.display()
        ),
    )
    .unwrap();

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("another participant's workspace"));
}

#[test]
fn a_participant_may_aggregate_into_their_own_workspace() {
    // **The other half of op's answer, and the reason deny item one is not an
    // absolute here.** Op's own workspace is one participant's: denied
    // to every other participant, and permitted to its owner, because nobody is
    // denied their own.
    //
    // The workspace sits at `<home>/Dev/clause-dev` deliberately. That is the
    // one path the home-derived deny list hard-codes, so it is the only fixture
    // that reaches the branch this test is about; a workspace anywhere else
    // passes without the permission existing at all, which is what the previous
    // fixture did while its comment described this case.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();

    let mine = home.join("Dev").join("clause-dev");
    std::fs::create_dir_all(&mine).unwrap();

    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            r#"
[workspace]
name = "ws"
path = "{}"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"

[org.me]
role = "hand"
staffed = true
handle = "me"
git_name = "me"
git_email = "me@example.invalid"
workspace = "{}"
"#,
            mine.display(),
            mine.display()
        ),
    )
    .unwrap();

    let out = bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "agent", "regen"])
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8_lossy(&out.stderr);

    // succeeded, rather than merely not having said one particular sentence. A
    // run that fails for any other reason also omits it.
    assert!(
        out.status.success(),
        "aggregating into a participant's own workspace failed: {stderr}"
    );
    // and it wrote, rather than succeeding by finding nothing to do.
    assert!(
        mine.join(".claude").join("settings.json").is_file(),
        "the pass reported success and wrote no settings: {stderr}"
    );
}

/// A workspace with one Rust repo declared and one shared config to hand out.
fn workspace_with_a_repo_and_a_config(dir: &std::path::Path) -> (std::path::PathBuf, String) {
    let ws = dir.join("workspace");
    std::fs::create_dir_all(ws.join(".shared").join("configs")).unwrap();
    std::fs::write(
        ws.join(".shared").join("configs").join("rustfmt.toml"),
        "max_width = 100\n",
    )
    .unwrap();

    let repo = ws.join("arvo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"arvo\"\n").unwrap();

    let cfg = format!(
        "{}\n[repos.arvo]\nforge = \"github\"\nowner = \"orgrinrt\"\nlocal_path = \"arvo\"\n",
        config_at(&ws)
    );
    (ws, cfg)
}

#[test]
fn regen_places_a_missing_shared_config_and_leaves_it_there() {
    // End to end through the built binary, because the unit tests exercise the
    // stage and say nothing about whether `agent regen` actually calls it.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let (ws, cfg_body) = workspace_with_a_repo_and_a_config(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, cfg_body).unwrap();

    let placed = ws.join("arvo").join("rustfmt.toml");
    assert!(!placed.exists(), "the fixture starts without the config");

    bin()
        .env("HOME", &home)
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "agent",
            "regen",
            "--skip-cargo-mock",
            "--continue-on-error",
        ])
        .output()
        .expect("the binary runs");

    assert_eq!(
        std::fs::read_to_string(&placed).unwrap(),
        "max_width = 100\n",
        "regen did not place the shared config"
    );
}

#[test]
fn a_second_regen_reports_the_config_as_matching_rather_than_placing_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let (ws, cfg_body) = workspace_with_a_repo_and_a_config(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, cfg_body).unwrap();

    let run = || {
        bin()
            .env("HOME", &home)
            .args([
                "--config",
                cfg.to_str().unwrap(),
                "agent",
                "regen",
                "--skip-cargo-mock",
                "--continue-on-error",
            ])
            .output()
            .expect("the binary runs")
    };

    let first = String::from_utf8_lossy(&run().stdout).to_string();
    assert!(first.contains("placed rustfmt.toml"), "first run: {first}");

    let second = String::from_utf8_lossy(&run().stdout).to_string();
    assert!(
        second.contains("rustfmt.toml matches"),
        "second run should be a no-op: {second}"
    );
    assert!(
        !second.contains("placed rustfmt.toml"),
        "the second run placed it again: {second}"
    );
    let _ = ws;
}

#[test]
fn a_repo_whose_config_differs_is_warned_about_and_the_run_still_succeeds() {
    // The asymmetry, end to end: a difference may be deliberate, so it is
    // reported, the file is left alone, and the exit status stays clean.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let (ws, cfg_body) = workspace_with_a_repo_and_a_config(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, cfg_body).unwrap();

    let theirs = ws.join("arvo").join("rustfmt.toml");
    std::fs::write(&theirs, "max_width = 80\n").unwrap();

    let out = bin()
        .env("HOME", &home)
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "agent",
            "regen",
            "--skip-cargo-mock",
            "--skip-aggregate",
            "--continue-on-error",
        ])
        .output()
        .expect("the binary runs");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("differs from the shared copy"),
        "the divergence was not reported: {stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(&theirs).unwrap(),
        "max_width = 80\n",
        "the repo's own config was overwritten"
    );
}

#[test]
fn skipping_every_stage_is_refused_but_skipping_two_is_not() {
    // The guard counts stages, and there are three. It counted two until the
    // configs stage arrived, at which point it refused a run that does real
    // work: the test above skips both of the others and depends on this.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let (_ws, cfg_body) = workspace_with_a_repo_and_a_config(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, cfg_body).unwrap();

    let run = |extra: &[&str]| {
        let mut args = vec!["--config", cfg.to_str().unwrap(), "agent", "regen"];
        args.extend_from_slice(extra);
        bin()
            .env("HOME", &home)
            .args(args)
            .output()
            .expect("the binary runs")
    };

    let all_three = run(&["--skip-cargo-mock", "--skip-configs", "--skip-aggregate"]);
    assert!(
        String::from_utf8_lossy(&all_three.stderr).contains("would do nothing"),
        "skipping every stage should be refused"
    );

    let two = run(&["--skip-cargo-mock", "--skip-aggregate", "--continue-on-error"]);
    let stderr = String::from_utf8_lossy(&two.stderr);
    assert!(
        !stderr.contains("would do nothing"),
        "skipping two of three still runs the configs stage: {stderr}"
    );
    assert!(
        String::from_utf8_lossy(&two.stdout).contains("rustfmt.toml"),
        "the configs stage did not run"
    );
}
