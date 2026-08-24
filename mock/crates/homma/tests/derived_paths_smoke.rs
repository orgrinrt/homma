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

    for pass in 1 ..= 2 {
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

/// A home with a live `.claude/` in it, which is every machine that runs an
/// agent. The previous tests built a home that did not exist, which is the one
/// configuration where the parent guard fires and therefore the one that hid
/// this.
fn a_home_with_a_real_claude(dir: &std::path::Path) -> std::path::PathBuf {
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(home.join(".claude").join("settings.json"), "{}").unwrap();
    home
}

#[test]
fn a_root_inside_the_operators_claude_directory_is_refused() {
    // The fourteenth review's reproduction, and the first defect on this branch
    // that containment could never have caught: the root contained every write
    // correctly, and the root was the problem.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    let inside = home.join(".claude").join("crewroot");
    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", inside.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(!inside.exists(), "and the root must not have been created");
}

#[test]
fn a_root_that_is_the_home_itself_is_refused_once_it_writes_into_claude() {
    // The shape where `.claude` already exists and homma writes its definitions
    // and memory link straight into it, beside the operator's settings. Every
    // write passed containment, because a home contains itself.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let cfg = home.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !home.join(".claude").join("agents").exists(),
        "no definition may land beside the operator's own settings"
    );
    assert!(!home.join(".claude").join("agent-memory").exists());
}

#[test]
fn a_workspace_under_an_existing_claude_directory_is_refused() {
    // The `.claude`-present variant of the path-building test above. There the
    // home was absent so the parent guard fired; here it exists, which is the
    // real case, and only the deny list stops it.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
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
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !home.join(".claude").join("hands").exists(),
        "not even the one level `create_dir` would make"
    );
}

#[test]
fn a_root_in_ops_own_workspace_is_refused() {
    // Deny item one, which has the same shape and had the same absence of a
    // mechanism.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let central = home.join("Dev").join("clause-dev");
    std::fs::create_dir_all(&central).unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", central.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yours to write in"));
}

#[test]
fn a_workspace_whose_path_is_missing_leaves_no_git_behind() {
    // `containment_smoke.rs` asserts this property for the sibling lexical
    // refusal. The parent refusal was added a round later without one, and it
    // fired inside `provision`, which runs after `git.init` creates the root, so
    // it left a `.git` against the comment three lines above it.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(
        &cfg,
        "r",
        dir.path()
            .join("nested")
            .join("deeper")
            .join("r")
            .to_str()
            .unwrap(),
    );

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("make"));

    assert!(
        !root.join(".git").exists(),
        "a refusal must leave nothing half-built, which is what the code claims"
    );
    assert!(!dir.path().join("nested").exists());
}

#[test]
fn a_differently_cased_root_inside_the_operators_claude_is_refused() {
    // **The fifteenth review's reproduction, and one character is the whole
    // exploit.** With `--root $HOME/.CLAUDE/crewroot` this exited 0 and wrote a
    // workspace, a repository, both definitions and the memory link into the
    // operator's own `.claude`, beside the credentials the record names, with
    // the deny list active and every containment proof satisfied. `starts_with`
    // compares components and this filesystem folds case.
    //
    // It establishes that the two spellings reach one directory before asserting
    // anything, so on a case-sensitive filesystem it reports that and stops
    // rather than passing for a reason unrelated to the guard.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());

    let folded = home.join(".CLAUDE");
    let (Ok(a), Ok(b)) = (
        std::fs::metadata(&folded),
        std::fs::metadata(home.join(".claude")),
    ) else {
        eprintln!("skipped: this filesystem is case-sensitive, so there is nothing to catch");
        return;
    };
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            (a.dev(), a.ino()),
            (b.dev(), b.ino()),
            "the two spellings must reach one inode for this to test anything"
        );
    }

    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    let inside = folded.join("crewroot");
    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", inside.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !home.join(".claude").join("crewroot").exists(),
        "and nothing may have been created under the directory the two spellings share"
    );
}

#[test]
fn a_root_inside_another_participants_workspace_is_refused() {
    // Deny item two, end to end, which had no test anywhere in the production
    // path: no fixture built a registry with two staffed participants, so
    // deleting the derivation left the whole suite green.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    let theirs = dir.path().join("ws-a");
    std::fs::create_dir_all(&theirs).unwrap();
    add_hand(&cfg, "a", theirs.to_str().unwrap());
    add_hand(&cfg, "b", dir.path().join("ws-b").to_str().unwrap());

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "b"])
        .args(["--root", theirs.join("subroot").to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("another participant's workspace"));

    assert!(!theirs.join("subroot").exists());
}

#[test]
fn a_root_inside_the_standees_own_workspace_is_refused_before_anything_is_built() {
    // The skip that made deny item two miss the standee itself. A root inside
    // its own workspace passed the list, `git.init` ran, and the run failed in
    // `provision` having left a `.git` behind: the third instance on this branch
    // of a refusal leaving something half-built, and the first inside the guard
    // added to close the list.
    let dir = tempfile::tempdir().unwrap();
    let home = a_home_with_a_real_claude(dir.path());
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    let own = dir.path().join("ws-own");
    std::fs::create_dir_all(&own).unwrap();
    add_hand(&cfg, "r", own.to_str().unwrap());

    let inside = own.join("subroot");
    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", inside.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !inside.join(".git").exists(),
        "a refusal leaves nothing half-built, and this is where that stopped being true"
    );
    assert!(!inside.exists());
}

/// Whether this filesystem folds case, established rather than assumed.
///
/// Creates a directory, stats a differently-cased spelling of it, and compares
/// `(dev, ino)`. The tests below use it to skip rather than to pass, because a
/// deny test that cannot tell a real refusal from an inapplicable one is the
/// shape this branch has shipped repeatedly.
fn folds_case(at: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let probe = at.join("CaseProbe");
    if std::fs::create_dir_all(&probe).is_err() {
        return false;
    }
    let folded = at.join("caseprobe");
    let same = match (std::fs::metadata(&probe), std::fs::metadata(&folded)) {
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
        _ => false,
    };
    let _ = std::fs::remove_dir(&probe);
    same
}

/// A home with no `.claude` in it, which is the condition the round before this
/// one could not answer for.
fn a_home_with_no_claude_yet(dir: &std::path::Path) -> std::path::PathBuf {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    assert!(!home.join(".claude").exists());
    home
}

// **The region the previous round's tests never entered.** `under_by_identity`
// answers nothing about a directory with no inode, so for an absent denied place
// the component comparison was the only arm running, and an exact one is what
// folding defeats. Every end-to-end deny test above pre-creates the denied
// directory, and the one test that entered this path used the identical
// spelling, so the broken region went unnamed in the round whose subject was
// this comparison.

#[test]
fn a_workspace_in_a_folded_spelling_of_a_claude_that_does_not_exist_yet_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    if !folds_case(dir.path()) {
        eprintln!("skipped: case-sensitive filesystem, the two spellings are two directories");
        return;
    }
    let home = a_home_with_no_claude_yet(dir.path());
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", home.join(".CLAUDE").join("ws").to_str().unwrap());

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing may be written there"));

    assert!(
        !home.join(".claude").exists() && !home.join(".CLAUDE").exists(),
        "the denied directory must not have been created on the way to refusing"
    );
}

#[test]
fn a_workspace_in_a_folded_spelling_of_a_denied_place_that_does_not_exist_yet_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    if !folds_case(dir.path()) {
        eprintln!("skipped: case-sensitive filesystem");
        return;
    }
    let home = a_home_with_no_claude_yet(dir.path());
    // `Dev` exists and `clause-dev` does not, which is the condition under test.
    // Without this the parent-must-exist guard refuses first and the deny check
    // is never reached, so the test would pass while saying nothing.
    std::fs::create_dir_all(home.join("Dev")).unwrap();
    // The root is a sibling of the home, so the workspace under the home is not
    // also inside the root; that guard would otherwise refuse first.
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(
        &cfg,
        "r",
        home.join("Dev")
            .join("CLAUSE-DEV")
            .join("ws")
            .to_str()
            .unwrap(),
    );

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yours to write in"));

    assert!(!home.join("Dev").join("clause-dev").exists());
    assert!(!home.join("Dev").join("CLAUSE-DEV").exists());
}

#[test]
fn a_workspace_inside_another_participants_workspace_that_is_not_stood_up_yet_is_refused() {
    // The ordinary case rather than a contrivance: a participant is declared
    // long before anybody stands them up, so their workspace routinely does not
    // exist when the next one is added.
    let dir = tempfile::tempdir().unwrap();
    if !folds_case(dir.path()) {
        eprintln!("skipped: case-sensitive filesystem");
        return;
    }
    let home = a_home_with_no_claude_yet(dir.path());
    // The root is the configuration's own directory, and a workspace may not sit
    // inside it, so both live in a sibling. Otherwise that guard refuses first
    // and this test never reaches the deny list.
    let root = dir.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(dir.path().join("out")).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();

    let theirs = dir.path().join("out").join("ws-a");
    assert!(!theirs.exists(), "declared and not yet stood up");
    add_hand(&cfg, "a", theirs.to_str().unwrap());
    add_hand(
        &cfg,
        "b",
        dir.path()
            .join("out")
            .join("WS-A")
            .join("inner")
            .to_str()
            .unwrap(),
    );

    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("another participant's workspace"));

    assert!(!theirs.exists() && !dir.path().join("out").join("WS-A").exists());
}

#[test]
fn a_root_in_a_folded_spelling_of_an_absent_denied_place_builds_nothing_before_refusing() {
    // **The refusal fired after the write it forbids.** `Root::new` passed
    // because the denied place had no inode, `git.init` created it through
    // `create_dir_all`, `provision` cloned the workspace, and only then did
    // `Layout::new` refuse, because by that point the directory existed and had
    // one. The guard succeeded precisely because the forbidden write had already
    // happened, which is the fourth time on this branch that "a refusal leaves
    // nothing half-built" has needed repairing.
    let dir = tempfile::tempdir().unwrap();
    if !folds_case(dir.path()) {
        eprintln!("skipped: case-sensitive filesystem");
        return;
    }
    let home = a_home_with_no_claude_yet(dir.path());
    // `Dev` exists and `clause-dev` does not: the condition under test. Without
    // it the parent-must-exist guard refuses first, which is a correct refusal
    // for the wrong reason and would leave this asserting nothing.
    std::fs::create_dir_all(home.join("Dev")).unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    add_hand(&cfg, "r", dir.path().join("ws").to_str().unwrap());

    let folded_root = home.join("Dev").join("CLAUSE-DEV");
    bin()
        .env("HOME", &home)
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "r"])
        .args(["--root", folded_root.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yours to write in"));

    let central = home.join("Dev").join("clause-dev");
    assert!(!central.exists(), "the denied place must not exist");
    assert!(!folded_root.exists());
    assert!(
        !dir.path().join("ws").exists(),
        "and the workspace must not have been cloned before the refusal"
    );
}
