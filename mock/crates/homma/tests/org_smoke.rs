//! End-to-end tests for `homma org`: adding identities and standing them up.
//!
//! Split from `cli_smoke.rs`, which crossed the file-size limit as these grew.
//! The lint scopes to `src/` and does not see a test file, so this one is on
//! discipline rather than on the gate.
//!
//! These run the real binary against real repositories on disk, cloned from
//! local paths rather than over the network. The unit tests use fakes; these
//! exist because a fake answers by construction the only question that matters
//! about a clone.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn bin() -> Command {
    Command::cargo_bin("homma").expect("binary built")
}

/// A registry with a comment and one entry, to add to.
fn registry_with_a_comment(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("homma.toml");
    std::fs::write(
        &path,
        "# a comment that must survive\ncontent_repo = \"git@example.invalid:orgrinrt/clause-dev.git\"\n\n\
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
    std::fs::write(
        root.join("homma.toml"),
        format!("content_repo = \"{}\"\n", src.display()),
    )
    .unwrap();
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
    std::fs::write(
        &cfg,
        "content_repo = \"git@example.invalid:x/content.git\"\n",
    )
    .unwrap();
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

/// Every place git would read a configuration that is not a repository's own.
fn global_config_paths() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/etc/gitconfig")];
    if let Ok(v) = std::env::var("GIT_CONFIG_GLOBAL") {
        out.push(PathBuf::from(v));
    }
    if let Ok(v) = std::env::var("XDG_CONFIG_HOME") {
        out.push(PathBuf::from(v).join("git").join("config"));
    }
    if let Ok(v) = std::env::var("HOME") {
        out.push(PathBuf::from(&v).join(".gitconfig"));
        out.push(PathBuf::from(&v).join(".config").join("git").join("config"));
    }
    out
}

#[test]
fn standing_up_changes_no_global_git_configuration() {
    // The half of the roadmap's exit test the integration test omitted. The
    // requirement is "in that clone's own config, never globally", and only the
    // first half was ever checked at this level.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    let ws = dir.path().join("ws").join("fresh");

    let globals = global_config_paths();
    assert!(!globals.is_empty(), "nothing to compare");
    let before: Vec<_> = globals.iter().map(|p| std::fs::read(p).ok()).collect();

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
        .assert()
        .success();

    let after: Vec<_> = globals.iter().map(|p| std::fs::read(p).ok()).collect();
    assert_eq!(
        before, after,
        "no global configuration may change: {globals:?}"
    );
}

#[test]
fn standing_up_does_not_depend_on_where_the_operator_is_standing() {
    // Reproduced before this existed: `--root` defaulted to the current
    // directory rather than the configuration's own, so running this from an
    // unrelated clone cloned that repository into the Hand's workspace and
    // wrote the Hand's directories into the unrelated clone's tree.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    let cfg = root.join("homma.toml");
    let ws = dir.path().join("ws").join("fresh");

    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();

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
        .current_dir(&elsewhere)
        .assert()
        .success();

    // The workspace is a clone of the configured content repository.
    assert!(ws.join(".git").exists());
    assert!(
        std::fs::read_to_string(ws.join("README.md"))
            .unwrap()
            .contains("content"),
        "the configured repository must be what was cloned"
    );

    // And nothing was written where the operator happened to be standing.
    assert!(
        !elsewhere.join(".shared").exists() && !elsewhere.join(".claude").exists(),
        "the current directory is not the workspace and must not be written to"
    );
    // It went to the configuration's own directory instead.
    assert!(root.join(".shared/hands/fresh").exists());
}

#[test]
fn a_registry_with_no_content_repo_treats_its_own_directory_as_one() {
    // `local` is the default, so a workspace needs no configuration key at all
    // to start: the directory holding the registry becomes the content
    // repository, initialised if it is not one yet.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "# no content_repo key\n").unwrap();
    let ws = dir.path().join("hands").join("fresh");

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
        .assert()
        .success();

    assert!(root.join(".git").exists(), "the root must be initialised");
    assert!(ws.join(".git").exists(), "and the workspace cloned from it");
    let local = std::fs::read_to_string(ws.join(".git/config")).unwrap();
    assert!(local.contains("fresh@example.invalid"));
}

#[test]
fn local_may_be_said_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    std::fs::create_dir_all(&root).unwrap();
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("b");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "b"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "b", "--git-email", "b@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "b"])
        .assert()
        .success();
    assert!(ws.join(".git").exists());
}

#[test]
fn a_local_root_that_is_already_a_repository_is_not_reinitialised() {
    // Standing up twice is the same answer, and initialising over an existing
    // repository would discard its history.
    let dir = tempfile::tempdir().unwrap();
    let (_src, root) = content_repo_and_root(dir.path());
    // Point it at itself rather than at the remote it was cloned from.
    let cfg = root.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let before = std::fs::read_to_string(root.join(".git/config")).unwrap();
    let ws = dir.path().join("hands").join("c");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "c"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "c", "--git-email", "c@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();
    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "c"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(root.join(".git/config")).unwrap(),
        before,
        "an existing repository must not be re-initialised"
    );
}

#[test]
fn a_symlinked_registry_is_written_through_rather_than_replaced() {
    // Renaming over a symlink replaces the link with a regular file: the entry
    // lands on the link, the file it pointed at never sees it, and the operator
    // maintains a registry that is silently stale beside a divergent copy.
    let dir = tempfile::tempdir().unwrap();
    let real_dir = dir.path().join("real");
    let link_dir = dir.path().join("link");
    std::fs::create_dir_all(&real_dir).unwrap();
    std::fs::create_dir_all(&link_dir).unwrap();

    let real = real_dir.join("homma.toml");
    std::fs::write(&real, "content_repo = \"local\"\n").unwrap();
    let link = link_dir.join("homma.toml");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    bin()
        .args(["--config", link.to_str().unwrap(), "org", "add", "victim"])
        .args(["--role", "hand"])
        .assert()
        .success();

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link must survive being written through"
    );
    assert!(
        std::fs::read_to_string(&real)
            .unwrap()
            .contains("org.victim"),
        "the entry must reach the file the link points at"
    );
}

#[test]
fn a_local_root_inside_another_repository_is_refused() {
    // Otherwise `local` initialises a repository inside somebody else's
    // checkout and writes a participant's directories into their tree, which
    // the deny list forbids outright.
    let dir = tempfile::tempdir().unwrap();
    let (_src, outer) = content_repo_and_root(dir.path());

    let nested = outer.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    let cfg = nested.join("homma.toml");
    std::fs::write(&cfg, "content_repo = \"local\"\n").unwrap();
    let ws = dir.path().join("hands").join("h");

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "add", "h"])
        .args(["--role", "hand", "--staffed"])
        .args(["--git-name", "h", "--git-email", "h@example.invalid"])
        .args(["--workspace", ws.to_str().unwrap()])
        .assert()
        .success();

    bin()
        .args(["--config", cfg.to_str().unwrap(), "org", "up", "h"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sits inside the repository"));

    assert!(
        !nested.join(".git").exists(),
        "no repository may be initialised inside another one"
    );
    assert!(!ws.exists(), "and nothing may be cloned");
}
