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
    // absolute here.** The central clone is one participant's workspace: denied
    // to every other participant, and permitted to its owner, because nobody is
    // denied their own. Without this the previous round's shape refused the
    // configuration that actually ships and `agent regen` exited 1 on every real
    // invocation.
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();

    let mine = dir.path().join("mine");
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
    assert!(
        !stderr.contains("nothing may be written there"),
        "a participant's own workspace is not denied to them: {stderr}"
    );
}
