//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The seam U-3.1 names, which nothing was testing.
//!
//! Its exit test says "A commit made in a **provisioned** workspace reports
//! author `ort@hiisi.digital` and committer `orgrinrt+vouti@ikiuni.dev`, read
//! back from the commit". Everything shipped so far tested one half or the
//! other: `provision` against a fake `Git`, and `GixGit` by calling
//! `set_identity` directly. The registry's committer never travelled the whole
//! way to a commit, so the defaulting in `provision` and the six config keys in
//! `GixGit` were joined by nothing.
//!
//! In `tests/` rather than beside `provision`, because it needs a concrete git
//! implementation and nothing in this crate's own code may reach one.

use homma_api::{AbsPath, Identity, Role};
use homma_core::repo::GixGit;
use homma_core::testing::hostile_git_identity_env;
use homma_org::provision;

fn run(args: &[&str], at: &std::path::Path, hostile: &std::path::Path) -> String {
    let mut git = std::process::Command::new("git");
    // The environment tier of the same hostility, for the identity under test.
    hostile_git_identity_env(&mut git);
    let out = git
        .args(args)
        .current_dir(at)
        .env("GIT_CONFIG_GLOBAL", hostile)
        .env("GIT_CONFIG_SYSTEM", hostile)
        .output()
        .expect("git should run");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A global configuration naming somebody else, for every key homma writes.
///
/// Without this the machine answers the test: an earlier version asserted the
/// author was `ort@hiisi.digital`, which was the author machine's own global
/// `user.email`, and deleting every author write left it passing.
fn hostile_config(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("hostile.gitconfig");
    std::fs::write(
        &p,
        "[user]\n\tname = Somebody Else\n\temail = wrong@example.invalid\n\
         [author]\n\tname = Wrong Author\n\temail = wrong-author@example.invalid\n\
         [committer]\n\tname = Wrong Committer\n\temail = wrong-committer@example.invalid\n",
    )
    .unwrap();
    p
}

// FIXME: the design does not say whether a provisioned workspace holds its identity
// against the environment. It does not hold it today: with the author and committer
// variables set, the commit carries the environment's. Open topic
// `202610231408_topic.how-a-provisioned-workspace-defends-its-identity-against-the-environment.md`.
#[test]
#[ignore = "catalogue: a provisioned workspace commits as the environment's identity when the author and committer variables are set; tracked 202610231408_topic.how-a-provisioned-workspace-defends-its-identity-against-the-environment"]
fn a_commit_in_a_provisioned_workspace_carries_the_registrys_two_identities() {
    let d = tempfile::tempdir().unwrap();
    let hostile = hostile_config(d.path());

    // Something to clone from, so `provision` takes its cloning path.
    let src = d.path().join("content");
    std::fs::create_dir_all(&src).unwrap();
    run(&["init", "-q", "-b", "main"], &src, &hostile);
    run(&["config", "user.name", "src"], &src, &hostile);
    run(
        &["config", "user.email", "src@example.invalid"],
        &src,
        &hostile,
    );
    std::fs::write(src.join("README.md"), "content").unwrap();
    run(&["add", "README.md"], &src, &hostile);
    run(
        &["commit", "-q", "-m", "initial", "--no-gpg-sign"],
        &src,
        &hostile,
    );

    let ws = d.path().join("crew").join("vouti");
    std::fs::create_dir_all(ws.parent().unwrap()).unwrap();
    let ws_abs = AbsPath::new(&ws).unwrap();

    // The registry entry, with the two identities the record settles for Vouti.
    let mut id = Identity::new(Role::Hand, "onni");
    id.staffed = true;
    id.workspace = Some(ws.to_string_lossy().into_owned());
    id.git_name = Some("Onni Armas".into());
    id.git_email = Some("ort@hiisi.digital".into());
    id.committer_name = Some("Vouti".into());
    id.committer_email = Some("orgrinrt+vouti@ikiuni.dev".into());

    provision(&id, &ws_abs, src.to_str().unwrap(), &GixGit).expect("provisioning succeeds");

    std::fs::write(ws.join("a"), "x").unwrap();
    run(&["add", "a"], &ws, &hostile);
    run(
        &["commit", "-q", "-m", "one", "--no-gpg-sign"],
        &ws,
        &hostile,
    );

    assert_eq!(
        run(&["log", "-1", "--format=%an"], &ws, &hostile),
        "Onni Armas",
        "the author's name travels from the registry to the commit"
    );
    assert_eq!(
        run(&["log", "-1", "--format=%ae"], &ws, &hostile),
        "ort@hiisi.digital",
        "the author stays op, per the record"
    );
    assert_eq!(
        run(&["log", "-1", "--format=%cn"], &ws, &hostile),
        "Vouti",
        "and the committer's name is the registry's, not the author's"
    );
    assert_eq!(
        run(&["log", "-1", "--format=%ce"], &ws, &hostile),
        "orgrinrt+vouti@ikiuni.dev",
        "and the committer address is what distinguishes the crew's writes"
    );
}

// FIXME: the same open topic as the test above; the ordinary entry fails the same way.
#[test]
#[ignore = "catalogue: a provisioned workspace commits as the environment's identity when the author and committer variables are set; tracked 202610231408_topic.how-a-provisioned-workspace-defends-its-identity-against-the-environment"]
fn an_entry_with_one_identity_commits_as_that_one_throughout() {
    // The ordinary entry, through the same seam. `provision` defaults the
    // committer to the author, and that defaulting was tested only against a
    // fake.
    let d = tempfile::tempdir().unwrap();
    let hostile = hostile_config(d.path());

    let src = d.path().join("content");
    std::fs::create_dir_all(&src).unwrap();
    run(&["init", "-q", "-b", "main"], &src, &hostile);
    run(&["config", "user.name", "src"], &src, &hostile);
    run(
        &["config", "user.email", "src@example.invalid"],
        &src,
        &hostile,
    );
    std::fs::write(src.join("README.md"), "content").unwrap();
    run(&["add", "README.md"], &src, &hostile);
    run(
        &["commit", "-q", "-m", "initial", "--no-gpg-sign"],
        &src,
        &hostile,
    );

    let ws = d.path().join("crew").join("paja");
    std::fs::create_dir_all(ws.parent().unwrap()).unwrap();
    let ws_abs = AbsPath::new(&ws).unwrap();

    let mut id = Identity::new(Role::Hand, "paja");
    id.staffed = true;
    id.workspace = Some(ws.to_string_lossy().into_owned());
    id.git_name = Some("Vaino Pajanen".into());
    id.git_email = Some("vaino.pajanen@hiisi.digital".into());

    provision(&id, &ws_abs, src.to_str().unwrap(), &GixGit).expect("provisioning succeeds");

    std::fs::write(ws.join("a"), "x").unwrap();
    run(&["add", "a"], &ws, &hostile);
    run(
        &["commit", "-q", "-m", "one", "--no-gpg-sign"],
        &ws,
        &hostile,
    );

    for format in ["--format=%an", "--format=%cn"] {
        assert_eq!(run(&["log", "-1", format], &ws, &hostile), "Vaino Pajanen");
    }
    for format in ["--format=%ae", "--format=%ce"] {
        assert_eq!(
            run(&["log", "-1", format], &ws, &hostile),
            "vaino.pajanen@hiisi.digital"
        );
    }
}
