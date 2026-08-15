//! That `Git::identity` reads the clone's own configuration, never git's merged
//! view.
//!
//! **Do not add a second test to this file.** The one below sets a process-wide
//! environment variable, and its safety rests entirely on being the only thing
//! in this process. A second test here reintroduces the race this file exists to
//! escape, silently and without a compiler complaint. A second binary beside it
//! is free.
//!
//! **Its own binary, and that is the point rather than an organisational
//! preference.** The test needs a global configuration to exist, so that
//! reporting one would be observably wrong, and the only way to guarantee that
//! on any machine is to write one and point `GIT_CONFIG_GLOBAL` at it.
//!
//! Setting an environment variable is process-wide. Inside the unit-test binary
//! that is a genuine race: `homma_core::testing::global_config_paths` reads the
//! same variable, `the_identity_is_written_to_the_local_config_file_and_nowhere_else`
//! calls it, and libtest runs the binary's tests concurrently. A round of this
//! branch set the variable there anyway, with a SAFETY note claiming a
//! single-threaded test, in a file whose own comment 140 lines below had already
//! refused to do it for exactly this reason.
//!
//! An integration test is its own process. The variable races with nothing, no
//! other test can observe it, and a panic cannot leave it set for anybody. The
//! race is not avoided here, it is unexpressible.

use homma_api::{AbsPath, CommitIdentity, Git};
use homma_core::repo::GixGit;

#[test]
fn the_clones_own_configuration_is_what_is_read() {
    let d = tempfile::tempdir().unwrap();

    // A global identity that would be reported if the merged view were read.
    // Without this the assertion below says nothing: on a machine with no global
    // configuration, an implementation reading the merged view reports `None`
    // too, and the test passes green against the thing it exists to catch.
    let global = d.path().join("global.gitconfig");
    std::fs::write(
        &global,
        "[user]\n\tname = Global Person\n\temail = global@example.invalid\n",
    )
    .unwrap();
    // SAFETY: this binary runs this test and nothing else, so no other thread
    // exists to observe the change. That is a property of the file's placement
    // rather than of this call, which is why the test lives here.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
    }

    let repo = d.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let at = AbsPath::new(&repo).unwrap();
    let git = GixGit;
    git.init(&at).unwrap();

    assert_eq!(
        git.identity(&at).unwrap(),
        None,
        "a fresh clone configures no identity, whatever the global one says"
    );

    // Empty is not absent on disk, and it is not a value either.
    let cfg = repo.join(".git").join("config");
    let mut text = std::fs::read_to_string(&cfg).unwrap();
    text.push_str("[user]\n\tname = \n\temail = \n");
    std::fs::write(&cfg, text).unwrap();

    assert_eq!(
        git.identity(&at).unwrap(),
        None,
        "an empty configured value is not a value"
    );

    // And a real local one is read, so the assertions above are not passing
    // because the method always answers `None`.
    git.set_identity(
        &at,
        &CommitIdentity {
            author_name: "Local Person".into(),
            author_email: "local@example.invalid".into(),
            committer_name: "Local Person".into(),
            committer_email: "local@example.invalid".into(),
        },
    )
    .unwrap();
    let got = git
        .identity(&at)
        .unwrap()
        .expect("a local identity is read");
    assert_eq!(got.author_email, "local@example.invalid");
    assert_ne!(
        got.author_email, "global@example.invalid",
        "the global one must never be what is reported"
    );
}
