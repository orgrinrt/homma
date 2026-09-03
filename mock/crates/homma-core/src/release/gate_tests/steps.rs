//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The steps one at a time: a failing call, docs reporting, deny and notices
//! behind their config files, the deno half, and the description step.

use super::*;

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
fn the_description_step_runs_in_the_gate_and_a_drifted_description_turns_the_run_red() {
    let readme = "# x\n\n<div align=\"center\">\n\n[![a](x)](y)\n\n> One line, said once.\n\n</div>\n\nOpening prose.\n";
    let fake = Fake::new(vec![]);
    let held = |description: &str| {
        let d = git_repo_with(&[
            ("README.md", readme),
            (
                "Cargo.toml",
                &format!(
                    "[package]\nname=\"x\"\nversion=\"0.1.0\"\ndescription=\"{description}\"\n"
                ),
            ),
        ]);
        run_gate(
            &fake,
            d.path(),
            &Markers::default(),
            "x",
            "2026-09-02T20:00:00Z",
        )
        .unwrap()
    };
    let green = held("One line, said once.");
    assert_eq!(green.verdict, Verdict::Green);
    let step = green
        .steps
        .iter()
        .find(|s| s.step == Step::Description)
        .expect("the gate ran the description step");
    assert!(step.passed && !step.skipped, "{}", step.log);

    let red = held("One line, said twice.");
    assert_eq!(
        red.verdict,
        Verdict::Red,
        "the description is a blocking step"
    );
    let step = red
        .steps
        .iter()
        .find(|s| s.step == Step::Description)
        .unwrap();
    assert!(!step.passed, "{}", step.log);
    assert!(
        step.log.contains("tagline:     One line, said once."),
        "{}",
        step.log
    );
    // the step runs no program: the runner saw nothing new for it
    assert!(
        !fake.seen.borrow().iter().any(|c| c.contains("README")),
        "{:?}",
        fake.seen.borrow()
    );
}
