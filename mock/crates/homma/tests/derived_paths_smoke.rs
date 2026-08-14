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

#[test]
fn a_dangling_symlink_on_a_definition_path_cannot_carry_the_write_out() {
    // The ninth review's reproduction, through the shipped binary. The shape the
    // test above does not reach: the link's *target* does not exist, so
    // `Path::exists()` on the link answers false and the old resolution took the
    // path as written. `fs::write` opens with `O_CREAT` and follows the link
    // anyway, so the definition landed in the other tree.
    //
    // With an absolute target this reaches another Hand's workspace, which is
    // deny item two, or `~/.claude`, which is item three.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let victim = dir.path().join("victim");
    std::fs::create_dir_all(root.join(".claude").join("agents")).unwrap();
    std::fs::create_dir_all(&victim).unwrap();
    std::os::unix::fs::symlink(
        victim.join("r.md"),
        root.join(".claude").join("agents").join("r.md"),
    )
    .unwrap();

    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the workspace root"));

    assert!(
        !victim.join("r.md").exists(),
        "the definition must not be written through the dangling link"
    );
}

#[test]
fn standing_up_twice_into_a_tree_with_a_symlink_is_the_same_answer() {
    // The tenth review's reproduction, and the one the idempotence laws could
    // not see because both run against a bare tempdir.
    //
    // `.claude -> .` is contained without argument: it resolves to the root
    // itself. It also removes a level from the real depth, and the memory link's
    // body was computed against the written depth, so the body climbed one level
    // too far and pointed outside.
    //
    // The second `org up` is what settles it. Homma's own guard, run against the
    // link homma created a command earlier, refused it: the crate contradicted
    // itself one command apart. So the assertion is not that a write is refused,
    // it is that the whole thing succeeds twice and the link stays inside.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(".", root.join(".claude")).unwrap();

    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    for pass in 1..=2 {
        bin()
            .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
            .assert()
            .success()
            .stderr(predicate::str::contains("outside the workspace root").not());
        let _ = pass;
    }

    // The link exists and what it points at is inside the root. Reading the body
    // rather than following it, because following it is what hid the defect: the
    // target was created, so `exists()` was happy while the path had left.
    let link = root.join(".claude").join("agent-memory").join("r");
    let body = std::fs::read_link(&link).expect("the memory link is created");
    let landed = std::fs::canonicalize(link.parent().unwrap().join(&body))
        .expect("the link's target resolves");
    let root_real = std::fs::canonicalize(&root).unwrap();
    assert!(
        landed.starts_with(&root_real),
        "the link body {} resolves to {}, outside the root {}",
        body.display(),
        landed.display(),
        root_real.display()
    );
}

#[test]
fn standing_up_into_a_root_whose_parent_is_missing_creates_nothing_above_it() {
    // The twelfth review's reproduction, exit 0 at the time. `create_dir_all`
    // creates every missing ancestor, so a missing root took its own ancestors
    // with it, and those sit above the root where containment cannot reach.
    //
    // The reviewer's second case is the one that matters: with the missing
    // prefix under a home directory, this created directories inside
    // `~/.claude/`, which the record forbids outright.
    let dir = tempfile::tempdir().unwrap();
    let cfgdir = dir.path().join("cfg");
    std::fs::create_dir_all(&cfgdir).unwrap();
    let cfg = cfgdir.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    let deep = dir.path().join("a").join("b").join("newroot");
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", deep.to_str().unwrap()])
        .assert()
        .failure();

    assert!(
        !dir.path().join("a").exists(),
        "nothing above the root may be created on the way to standing one up"
    );
}

#[test]
fn a_workspace_cannot_build_a_path_of_directories_to_reach_itself() {
    // The thirteenth review's reproduction. The workspace is required to sit
    // outside the containment root, so no `Root` covers it and none can; its
    // only guard asks whether it is inside a git repository, and a home
    // directory is not one.
    //
    // `create_dir_all` on its parent built whatever chain the configured path
    // implied, so a workspace at `somewhere/.claude/hands/paja` created
    // `somewhere/` and `somewhere/.claude/` on the way. That is deny item three.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    let home = dir.path().join("fakehome");
    add_hand(
        &cfg,
        "r",
        home.join(".claude")
            .join("hands")
            .join("r")
            .to_str()
            .unwrap(),
    );

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure();

    assert!(
        !home.exists(),
        "homma must not build a path of directories into a home to reach a workspace"
    );
}

#[test]
fn a_workspace_reached_by_climbing_out_cannot_build_its_path_either() {
    // The same defect spelled with `..` rather than absolutely, which is the
    // other spelling this branch has been closing since its first round.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("deep").join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    add_hand(&cfg, "r", "../../victimhome/.claude/hands/r");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure();

    assert!(
        !dir.path().join("victimhome").exists(),
        "climbing out must not create the tree it climbs into"
    );
}
