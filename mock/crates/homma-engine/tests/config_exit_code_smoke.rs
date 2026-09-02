//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What `homma repo config check` tells a shell, through the built binary.
//!
//! The commit gate runs that command and decides from its exit code alone, so
//! the code is the whole interface between the two. Every piece of it was
//! covered and the thing they compose to was not: a wrong `Ok` in the dispatch
//! arm would leave every other test green while the gate allowed every commit
//! in a workspace missing every config.
//!
//! So these assert the code rather than the wording. The wording is what a
//! person reads and is asserted where it is built; this is what the gate reads.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

const DENY: &str = "[bans]\nmultiple-versions = \"deny\"\n";

/// A workspace with one Rust member and the shared configs laid out under
/// `tag_dir`, which is empty for the untagged case.
///
/// The member is a real repository because that is what makes it a member:
/// homma finds members by looking for repositories under the root rather than
/// by reading a list, so a plain directory is not one and the command
/// correctly says there is no such member.
fn workspace(tag_dir: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let ws = dir.path().to_path_buf();
    let configs = if tag_dir.is_empty() {
        ws.join(".shared").join("configs")
    } else {
        ws.join(".shared").join("configs").join(tag_dir)
    };
    std::fs::create_dir_all(&configs).unwrap();
    std::fs::write(configs.join("deny.toml"), DENY).unwrap();
    // The `[workspace]` table is required, whatever the real manifest's own
    // comment says about `content_repo` being the only key homma needs. Without
    // it the parse fails, reported at line 1 column 1 rather than at the table
    // that is missing.
    std::fs::write(
        ws.join("homma.toml"),
        "content_repo = \"git@example.invalid:orgrinrt/clause-dev.git\"\n\n[workspace]\nname = \
         \"t\"\n",
    )
    .unwrap();

    let repo = ws.join("arvo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"arvo\"\n").unwrap();
    git_init(&repo);
    (dir, ws)
}

fn git_init(at: &Path) {
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(at)
        .status()
        .expect("git would not start");
    assert!(ok.success(), "git init failed in {}", at.display());
}

/// Run `repo config <verb>` for `arvo` and return the exit success plus stdout.
fn run(ws: &Path, verb: &str) -> (bool, String) {
    let out = bin()
        .args(["--dir", ws.to_str().unwrap(), "repo", "config", verb, "--repo", "arvo"])
        .output()
        .expect("the binary runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_repo_missing_a_required_config_exits_non_zero() {
    let (_dir, ws) = workspace("rust_required");
    let (ok, text) = run(&ws, "check");
    assert!(
        !ok,
        "a repo missing a required config exited zero, so the gate would allow the commit: {text}"
    );
    // The refusal a person sees has to say which repo and which config, or the
    // gate quotes an empty reason and stops them with no way to act.
    assert!(text.contains("arvo"), "{text}");
    assert!(text.contains("deny.toml"), "{text}");
}

#[test]
fn a_workspace_whose_configs_are_in_order_exits_zero() {
    // The control. Without it the case above would pass against a command that
    // exits non-zero unconditionally, which would refuse every commit in every
    // repo and look exactly like the gate working.
    let (_dir, ws) = workspace("rust_required");
    std::fs::write(ws.join("arvo").join("deny.toml"), DENY).unwrap();
    let (ok, text) = run(&ws, "check");
    assert!(ok, "a repo owing nothing was refused: {text}");
}

#[test]
fn init_places_it_and_the_check_then_passes() {
    let (_dir, ws) = workspace("rust_required");
    let (ok, text) = run(&ws, "init");
    assert!(ok, "placing what was missing failed: {text}");

    // Read back rather than trusting the report: a placement stage that
    // reported success and wrote nothing would satisfy an assertion about its
    // own output, which is the exact shape of a fail-open.
    let placed = ws.join("arvo").join("deny.toml");
    assert_eq!(
        std::fs::read_to_string(&placed).expect("the config was placed"),
        DENY,
        "the placed file is not the shared copy"
    );

    let (ok, text) = run(&ws, "check");
    assert!(
        ok,
        "the check still refuses after the config was placed: {text}"
    );
}

#[test]
fn an_untagged_template_does_not_block() {
    // The state a workspace is in before its templates move into tag
    // directories, and the reason this can land on its own. An untagged
    // template is a fault in the shared directory rather than in any repo, so
    // blocking would refuse every commit everywhere for something no repo can
    // fix.
    let (_dir, ws) = workspace("");
    let (ok, text) = run(&ws, "check");
    assert!(ok, "an untagged template stopped a commit: {text}");
    // Reported rather than passed over, because somebody still has to place it.
    assert!(text.contains("deny.toml"), "{text}");
}
