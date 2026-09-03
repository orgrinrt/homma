//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The feature-set matrix: which sets a crate or a workspace member is built
//! under, and which runs each step makes for them.

use super::*;

#[test]
fn a_crate_without_feature_sets_is_tested_with_all_and_with_none() {
    let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let fake = Fake::new(vec![(
        "cargo test",
        0,
        "test result: ok. 2 passed; 0 failed; 0 ignored\n",
    )]);
    let out = run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    assert!(out.passed && !out.skipped);
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo test --all-features",
        "cargo test --no-default-features"
    ]);
    assert_eq!(out.numbers["tests"], "4");
    assert_eq!(out.numbers["passed"], "4");
}

#[test]
fn feature_sets_declared_by_a_workspace_member_are_read_off_a_virtual_root() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("crates/inner")).unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[\"a\"]]\n",
    )
    .unwrap();
    assert_eq!(feature_sets(d.path()).unwrap(), vec![(
        Some("inner".to_string()),
        vec![vec!["a".to_string()]]
    )]);
    // and a member declaring none leaves the root's answer, which is none
    std::fs::write(
        d.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    assert!(feature_sets(d.path()).unwrap().is_empty());
}

#[test]
fn each_member_s_feature_sets_run_against_that_member_and_none_is_inherited() {
    let d = tempfile::tempdir().unwrap();
    for (name, sets) in [("alpha", "[[\"a\"]]"), ("zeta", "[[\"z\"], []]")] {
        std::fs::create_dir_all(d.path().join("crates").join(name)).unwrap();
        std::fs::write(
            d.path().join("crates").join(name).join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = {sets}\n"
            ),
        )
        .unwrap();
    }
    std::fs::create_dir_all(d.path().join("crates/plain")).unwrap();
    std::fs::write(
        d.path().join("crates/plain/Cargo.toml"),
        "[package]\nname = \"plain\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    // the workspace-wide runs, which leave out every member that declared its
    // own builds, then each such member's sets against itself; `plain`
    // declares none and inherits none, and `zeta`'s empty set is its own
    // no-features run
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo test --all-features --workspace --exclude alpha --exclude zeta",
        "cargo test --no-default-features --workspace --exclude alpha --exclude zeta",
        "cargo test -p alpha --no-default-features --features a",
        "cargo test -p zeta --no-default-features --features z",
        "cargo test -p zeta --no-default-features",
    ]);
}

#[test]
fn a_commit_that_is_not_the_head_is_gated_in_a_worktree_that_is_gone_after() {
    let d = git_repo_with(&[(
        "Cargo.toml",
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )]);
    let first = git::head(d.path()).unwrap();
    std::fs::write(d.path().join("f"), "x").unwrap();
    let g = |args: &[&str]| {
        let out = sh::run(d.path(), "git", args).unwrap();
        assert!(out.ok(), "{}", out.log());
    };
    g(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
    g(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "two"]);
    let head = git::head(d.path()).unwrap();
    assert_ne!(first, head, "the control: two commits");
    let fake = Fake::new(vec![]);
    let run = run_gate_at(&fake, d.path(), &Markers::default(), &first, "x", "t").unwrap();
    assert_eq!(run.sha, first, "the run measures the commit asked for");
    assert_eq!(
        git::head(d.path()).unwrap(),
        head,
        "the checkout did not move"
    );
    let out = sh::run(d.path(), "git", &["worktree", "list"]).unwrap();
    assert_eq!(
        out.stdout.lines().count(),
        1,
        "only the checkout remains: {}",
        out.stdout
    );
    // the head itself runs in place
    let run = run_gate_at(&fake, d.path(), &Markers::default(), &head, "x", "t").unwrap();
    assert_eq!(run.sha, head);
    // and a sha that is not there is refused rather than gated as nothing
    assert!(
        run_gate_at(
            &fake,
            d.path(),
            &Markers::default(),
            "0000000000000000000000000000000000000000",
            "x",
            "t"
        )
        .is_err()
    );
}

#[test]
fn feature_sets_from_the_manifest_each_get_a_run() {
    let d = crate_root(
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[], [\"a\"], [\"a\", \"b\"]]\n",
    );
    assert_eq!(feature_sets(d.path()).unwrap(), vec![(None, vec![
        vec![],
        vec!["a".to_string()],
        vec!["a".to_string(), "b".to_string()]
    ])]);
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    // the sets are the whole of it: no all-features run beside them
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo test --no-default-features",
        "cargo test --no-default-features --features a",
        "cargo test --no-default-features --features a,b"
    ]);
}

#[test]
fn a_root_declaring_sets_is_linted_and_documented_per_set_and_never_with_all() {
    let d = crate_root(
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[\"u8\"], [\"u16\", \"strict\"]]\n",
    );
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Lint).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo clippy --all-targets --no-default-features --features u8 -- -D warnings",
        "cargo clippy --all-targets --no-default-features --features u16,strict -- -D warnings",
    ]);
    fake.seen.borrow_mut().clear();
    run_step(&fake, d.path(), RepoKind::Crate, Step::Docs).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo doc --no-deps --no-default-features --features u8",
        "cargo doc --no-deps --no-default-features --features u16,strict",
    ]);
    // and on the tests step too, two features the crate declared apart are
    // never enabled together, which is what `--all-features` would have done
    fake.seen.borrow_mut().clear();
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    assert!(!fake.seen.borrow().is_empty());
    for line in fake.seen.borrow().iter() {
        assert!(!line.contains("--all-features"), "{line}");
        assert!(!(line.contains("u8") && line.contains("u16")), "{line}");
    }
}

#[test]
fn an_empty_declaration_is_a_manifest_error_and_no_step_runs() {
    let d = crate_root(
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = []\n",
    );
    assert!(matches!(
        feature_sets(d.path()),
        Err(GateError::Manifest(_))
    ));
    let fake = Fake::new(vec![]);
    for step in [Step::Lint, Step::Tests, Step::Docs] {
        assert!(matches!(
            run_step(&fake, d.path(), RepoKind::Crate, step),
            Err(GateError::Manifest(_))
        ));
    }
    assert!(fake.seen.borrow().is_empty(), "nothing ran, nothing passed");
    // the control: one named set is read, and the same steps run
    let d = crate_root(
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[\"a\"]]\n",
    );
    assert!(feature_sets(d.path()).is_ok());
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    assert_eq!(fake.seen.borrow().len(), 1);
}

#[test]
fn a_member_with_an_empty_declaration_is_refused_rather_than_excluded() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("crates/inner")).unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = []\n",
    )
    .unwrap();
    assert!(matches!(
        feature_sets(d.path()),
        Err(GateError::Manifest(_))
    ));
    let fake = Fake::new(vec![]);
    assert!(run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).is_err());
    assert!(fake.seen.borrow().is_empty());
}

/// Currently red, and deliberately left so.
///
/// A root package that declares sets is built once per set and the members
/// under it are built by nothing. The mirror case below, a root declaring
/// nothing beside a member that declares, handles its members carefully, so the
/// asymmetry is in the code rather than in either fixture.
///
/// Which behaviour is wanted is open. Either the members are reached, on the
/// reasoning the other branch already uses, or a root declaring sets has said
/// which builds are legal for the whole tree and a workspace-wide run would
/// perform one it excluded. This asserts the first, which is the intended
/// behaviour if the case is meant to work at all; it turns green if that is
/// chosen and is deleted with a sentence if the other is.
///
/// It cannot happen in this estate today, because a `None` entry needs the root
/// to be a package and every workspace here has a virtual root. It goes live
/// the first time one does not, and it fails by reporting green having built
/// less.
#[test]
#[ignore = "catalogue: a root package declaring feature sets builds no member at all; \
            which of the two readings is right is undecided, see design round 202609030600"]
fn a_root_declaring_sets_still_builds_its_members() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("crates/inner")).unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \
         \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[\"u8\"], \
         [\"u16\"]]\n[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo test --no-default-features --features u8",
        "cargo test --no-default-features --features u16",
        "cargo test --workspace --exclude outer --all-features",
    ]);
}

#[test]
fn a_root_package_keeps_its_bare_runs_beside_a_declaring_member() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("crates/inner")).unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\n[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("crates/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[\"a\"]]\n",
    )
    .unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    // no `--workspace`, so the root builds alone as before; the member's set
    // runs against the member
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo test --all-features",
        "cargo test --no-default-features",
        "cargo test -p inner --no-default-features --features a",
    ]);
    fake.seen.borrow_mut().clear();
    run_step(&fake, d.path(), RepoKind::Crate, Step::Lint).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo clippy --all-targets -p inner --no-default-features --features a -- -D warnings",
    ]);
}

#[test]
fn a_declaring_member_is_left_out_of_the_workspace_runs_on_every_step() {
    let d = tempfile::tempdir().unwrap();
    for (name, meta) in [
        (
            "alpha",
            "[package.metadata.homma]\nfeature_sets = [[\"a\"]]\n",
        ),
        ("plain", ""),
    ] {
        std::fs::create_dir_all(d.path().join("crates").join(name)).unwrap();
        std::fs::write(
            d.path().join("crates").join(name).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{meta}"),
        )
        .unwrap();
    }
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Lint).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo clippy --all-targets --all-features --workspace --exclude alpha -- -D warnings",
        "cargo clippy --all-targets -p alpha --no-default-features --features a -- -D warnings",
    ]);
    fake.seen.borrow_mut().clear();
    run_step(&fake, d.path(), RepoKind::Crate, Step::Docs).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo doc --no-deps --all-features --workspace --exclude alpha",
        "cargo doc --no-deps -p alpha --no-default-features --features a",
    ]);
}

#[test]
fn a_crate_declaring_no_sets_is_linted_and_documented_with_all_features_alone() {
    let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Lint).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo clippy --all-targets --all-features -- -D warnings"
    ]);
    fake.seen.borrow_mut().clear();
    run_step(&fake, d.path(), RepoKind::Crate, Step::Docs).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo doc --no-deps --all-features"
    ]);
}

#[test]
fn a_malformed_feature_set_is_a_manifest_error_not_a_skip() {
    let d = crate_root(
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [\"a\"]\n",
    );
    assert!(matches!(
        feature_sets(d.path()),
        Err(GateError::Manifest(_))
    ));
}
