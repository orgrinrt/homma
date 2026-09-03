//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The gate against a runner that answers from a table, so every step can be
//! exercised without cargo or deno installed.

use std::cell::RefCell;

use homma_api::Verdict;

use super::*;

/// Answers each command line from a table and records what was asked.
struct Fake {
    replies: Vec<(&'static str, i32, &'static str)>,
    seen:    RefCell<Vec<String>>,
}

impl Fake {
    fn new(replies: Vec<(&'static str, i32, &'static str)>) -> Self {
        Self {
            replies,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for Fake {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        let line = format!("{program} {}", args.join(" "));
        self.seen.borrow_mut().push(line.clone());
        let (status, stdout) = self
            .replies
            .iter()
            .find(|(prefix, ..)| line.starts_with(prefix))
            .map(|(_, s, o)| (*s, *o))
            .unwrap_or((0, ""));
        Ok(sh::Output {
            program: program.into(),
            args:    args.iter().map(|a| a.to_string()).collect(),
            status:  Some(status),
            stdout:  stdout.into(),
            stderr:  String::new(),
        })
    }
}

fn crate_root(manifest: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("Cargo.toml"), manifest).unwrap();
    d
}

fn git_repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (name, body) in files {
        std::fs::write(d.path().join(name), body).unwrap();
    }
    let run = |args: &[&str]| {
        let out = sh::run(d.path(), "git", args).unwrap();
        assert!(out.ok(), "{}", out.log());
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"]);
    d
}

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
    let run = run_gate_at(&fake, d.path(), &first, "x", "t").unwrap();
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
    let run = run_gate_at(&fake, d.path(), &head, "x", "t").unwrap();
    assert_eq!(run.sha, head);
    // and a sha that is not there is refused rather than gated as nothing
    assert!(
        run_gate_at(
            &fake,
            d.path(),
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

#[test]
fn a_failing_call_stops_the_step_and_turns_a_blocking_step_red() {
    let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let fake = Fake::new(vec![(
        "cargo test --all-features",
        101,
        "test result: FAILED. 1 passed; 1 failed; 0 ignored\n",
    )]);
    let out = run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
    assert!(!out.passed);
    assert!(out.is_red());
    assert_eq!(fake.seen.borrow().len(), 1);
    assert!(out.log.starts_with("$ cargo test --all-features\n"));
    assert_eq!(out.numbers["passed"], "1");
}

#[test]
fn docs_reports_its_fraction_and_never_blocks() {
    let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let fake = Fake::new(vec![("cargo doc", 1, "| Total | 3 | 75.0% | 0 |\n")]);
    let out = run_step(&fake, d.path(), RepoKind::Crate, Step::Docs).unwrap();
    assert!(!out.passed);
    assert!(!out.is_red());
    assert_eq!(out.numbers["documented_percent"], "75.0");
}

#[test]
fn deny_and_notices_run_only_when_their_config_exists() {
    let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let fake = Fake::new(vec![]);
    assert!(
        run_step(&fake, d.path(), RepoKind::Crate, Step::Deny)
            .unwrap()
            .skipped
    );
    assert!(
        run_step(&fake, d.path(), RepoKind::Crate, Step::Notices)
            .unwrap()
            .skipped
    );
    std::fs::write(d.path().join("deny.toml"), "").unwrap();
    std::fs::write(d.path().join("ante.toml"), "").unwrap();
    let fake = Fake::new(vec![("cargo deny", 1, "error[vulnerability]: x\n")]);
    let deny = run_step(&fake, d.path(), RepoKind::Crate, Step::Deny).unwrap();
    assert!(deny.is_red());
    assert_eq!(deny.numbers["advisories"], "1");
    let notices = run_step(&fake, d.path(), RepoKind::Crate, Step::Notices).unwrap();
    assert!(notices.passed && !notices.skipped);
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo deny check",
        "ante check"
    ]);
}

#[test]
fn a_deno_package_lints_then_checks_every_export_and_tests_through_its_task() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("deno.json"),
        r#"{"exports": {".": "./mod.ts", "./x": "./x.ts"}, "tasks": {"test": "deno test -A"}}"#,
    )
    .unwrap();
    let fake = Fake::new(vec![(
        "deno task test",
        0,
        "ok | 3 passed | 0 failed (1ms)\n",
    )]);
    let lint = run_step(&fake, d.path(), RepoKind::Deno, Step::Lint).unwrap();
    assert!(lint.passed);
    let tests = run_step(&fake, d.path(), RepoKind::Deno, Step::Tests).unwrap();
    assert_eq!(tests.numbers["tests"], "3");
    let seen = fake.seen.borrow();
    assert_eq!(seen[0], "deno lint");
    assert!(seen[1 .. 3].contains(&"deno check ./mod.ts".to_string()));
    assert!(seen[1 .. 3].contains(&"deno check ./x.ts".to_string()));
    assert_eq!(seen[3], "deno task test");
}

#[test]
fn a_deno_package_without_a_test_task_runs_deno_test_and_a_string_export_is_one_check() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("deno.json"), r#"{"exports": "./mod.ts"}"#).unwrap();
    assert_eq!(deno_exports(d.path()).unwrap(), vec!["./mod.ts"]);
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Deno, Step::Tests).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &["deno test"]);
}

#[test]
fn a_repo_that_is_both_runs_both_halves_of_a_step() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(d.path().join("deno.json"), "{}").unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Both, Step::Format).unwrap();
    assert_eq!(fake.seen.borrow().as_slice(), &[
        "cargo fmt --check",
        "deno fmt --check"
    ]);
}

#[test]
fn the_whole_gate_refuses_a_dirty_tree_and_runs_every_step_on_a_clean_one() {
    let d = git_repo_with(&[("Cargo.toml", "[package]\nname=\"x\"\nversion=\"0.1.0\"\n")]);
    let fake = Fake::new(vec![(
        "cargo test",
        0,
        "test result: ok. 1 passed; 0 failed; 0 ignored\n",
    )]);
    let run = run_gate(&fake, d.path(), "x", "2026-09-02T20:00:00Z").unwrap();
    assert_eq!(run.verdict, Verdict::Green);
    assert_eq!(run.steps.len(), Step::ALL.len());
    assert_eq!(run.sha, git::head(d.path()).unwrap());
    // the wall time sits on the last step that ran, not on a skipped one
    let carrier = run
        .steps
        .iter()
        .find(|s| s.numbers.contains_key("wall_seconds"))
        .expect("some step carries the wall time");
    assert!(!carrier.skipped, "{carrier:?}");
    assert!(
        run.steps
            .iter()
            .rev()
            .find(|s| !s.skipped)
            .is_some_and(|s| s.step == carrier.step),
        "it is the last step that ran"
    );
    assert!(run.steps.iter().any(|s| s.step == Step::Deny && s.skipped));
    std::fs::write(d.path().join("Cargo.toml"), "changed").unwrap();
    assert!(matches!(
        run_gate(&fake, d.path(), "x", "2026-09-02T20:00:00Z"),
        Err(GateError::Dirty)
    ));
}

#[test]
fn one_red_blocking_step_makes_the_run_red_while_a_failing_docs_step_does_not() {
    let d = git_repo_with(&[("Cargo.toml", "[package]\nname=\"x\"\nversion=\"0.1.0\"\n")]);
    let fake = Fake::new(vec![("cargo doc", 1, "")]);
    assert_eq!(
        run_gate(&fake, d.path(), "x", "t").unwrap().verdict,
        Verdict::Green
    );
    let fake = Fake::new(vec![("cargo fmt", 1, "Diff in src/lib.rs\n")]);
    let run = run_gate(&fake, d.path(), "x", "t").unwrap();
    assert_eq!(run.verdict, Verdict::Red);
    assert!(run.steps[0].log.contains("Diff in src/lib.rs"));
}

#[test]
fn a_tree_with_no_manifest_is_an_error_not_an_empty_green() {
    let d = git_repo_with(&[("README.md", "hi")]);
    let fake = Fake::new(vec![]);
    assert!(matches!(
        run_gate(&fake, d.path(), "x", "t"),
        Err(GateError::NoManifest(_))
    ));
}
