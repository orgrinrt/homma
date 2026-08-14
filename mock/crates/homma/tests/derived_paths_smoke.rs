//! Where the paths **derived from** a root may lead, end to end.
//!
//! A sibling of `containment_smoke.rs` rather than more of it, and the seam is
//! the one the eighth review named. That file covers the paths a caller hands
//! in: the root, the workspace, the configuration. This one covers the paths
//! homma computes from them, which is a different guard answering a different
//! question.
//!
//! The distinction is the whole content of the round these came from. Seven
//! rounds guarded a path somebody passed in and reported the class closed, and
//! the class was never in that half: `Layout` built every write target
//! lexically from the root, and lexical arithmetic cannot see through a
//! symlink. Keeping the two sets in one file is how the second one stayed
//! invisible while the first accumulated eight tests.

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
}

fn add_hand(cfg: &std::path::Path, handle: &str, workspace: &str) {
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", handle])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", handle, "--git-email", "h@example.invalid"])
        .args(["--workspace", workspace])
        .assert()
        .success();
}

#[test]
fn a_symlink_inside_the_root_cannot_carry_a_write_out_of_it() {
    // The eighth review's reproduction, which exited 0 and created
    // `victim/hands/r`. Every path `prepare` and `write_definitions` write to
    // was `root.join(paths.hands)`, computed and never checked, and `join` is
    // lexical by design.
    //
    // This is not an exotic filesystem. homma commits a symlink itself, for the
    // memory link, so a clone of the content repository is expected to carry
    // them, and the write lands under the first two deny items.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let victim = dir.path().join("victim");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&victim).unwrap();
    std::os::unix::fs::symlink("../victim", root.join("elsewhere")).unwrap();

    let cfg = root.join("homma.toml");
    // The configured directory is a plain relative path that escapes nothing:
    // `RelPath` accepts it, `join` clamps nothing, and the symlink does the rest.
    std::fs::write(
        &cfg,
        "content_repo = \"local\"\n\n[paths]\nhands = \"elsewhere/hands\"\n",
    )
    .unwrap();

    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the workspace root"));

    assert!(
        !victim.join("hands").exists(),
        "the write must not land in the tree the symlink points at"
    );
}

#[test]
fn a_workspace_inside_an_unrelated_repository_leaves_no_git_behind() {
    // The eighth review's second finding. The lexical check added in the
    // seventh round covers a workspace inside the *root*; a workspace nested in
    // some other repository fell through it, `git.init(root)` ran, and
    // `provision` refused afterwards.
    //
    // The `.git` left in the root is the assertion. A refusal that has already
    // half-built something is the defect, not the refusal.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    // An unrelated repository, entirely outside the root, with the workspace
    // nested inside it.
    let stranger = dir.path().join("stranger");
    std::fs::create_dir_all(&stranger).unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&stranger)
        .status()
        .expect("git should run")
        .success();
    assert!(ok, "git init failed");

    add_hand(&cfg, "r", stranger.join("nested").to_str().unwrap());

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sits inside the repository at"));

    assert!(
        !root.join(".git").exists(),
        "the root must not be initialised on the way to a refusal"
    );
    assert!(
        !stranger.join("nested").exists(),
        "and nothing may be created in the stranger's tree"
    );
}
