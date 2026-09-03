//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The whole gate: a dirty tree, the verdict, a tree with no manifest, and a
//! repository whose markers say only content.

use super::*;

#[test]
fn the_whole_gate_refuses_a_dirty_tree_and_runs_every_step_on_a_clean_one() {
    let d = git_repo_with(&[("Cargo.toml", "[package]\nname=\"x\"\nversion=\"0.1.0\"\n")]);
    let fake = Fake::new(vec![(
        "cargo test",
        0,
        "test result: ok. 1 passed; 0 failed; 0 ignored\n",
    )]);
    let run = run_gate(
        &fake,
        d.path(),
        &Markers::default(),
        "x",
        "2026-09-02T20:00:00Z",
    )
    .unwrap();
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
        run_gate(
            &fake,
            d.path(),
            &Markers::default(),
            "x",
            "2026-09-02T20:00:00Z"
        ),
        Err(GateError::Dirty)
    ));
}

#[test]
fn one_red_blocking_step_makes_the_run_red_while_a_failing_docs_step_does_not() {
    let d = git_repo_with(&[("Cargo.toml", "[package]\nname=\"x\"\nversion=\"0.1.0\"\n")]);
    let fake = Fake::new(vec![("cargo doc", 1, "")]);
    assert_eq!(
        run_gate(&fake, d.path(), &Markers::default(), "x", "t")
            .unwrap()
            .verdict,
        Verdict::Green
    );
    let fake = Fake::new(vec![("cargo fmt", 1, "Diff in src/lib.rs\n")]);
    let run = run_gate(&fake, d.path(), &Markers::default(), "x", "t").unwrap();
    assert_eq!(run.verdict, Verdict::Red);
    assert!(run.steps[0].log.contains("Diff in src/lib.rs"));
}

#[test]
fn a_tree_with_no_manifest_is_an_error_not_an_empty_green() {
    let d = git_repo_with(&[("README.md", "hi")]);
    let fake = Fake::new(vec![]);
    assert!(matches!(
        run_gate(&fake, d.path(), &Markers::default(), "x", "t"),
        Err(GateError::NoManifest(_))
    ));
}

#[test]
fn a_declared_content_marker_runs_a_gate_of_skipped_steps_and_passes() {
    use homma_api::Signal;
    let d = git_repo_with(&[("README.md", "hi"), ("polka.toml", "")]);
    let fake = Fake::new(vec![]);
    // the same tree under the bare defaults is still the error above
    assert!(matches!(
        run_gate(&fake, d.path(), &Markers::default(), "x", "t"),
        Err(GateError::NoManifest(_))
    ));
    let m = Markers::new([("polka.toml".to_string(), Signal::Content)]);
    let run = run_gate(&fake, d.path(), &m, "x", "t").unwrap();
    assert_eq!(run.verdict, Verdict::Green);
    assert_eq!(run.steps.len(), Step::ALL.len());
    assert!(run.steps.iter().all(|s| s.skipped), "{:?}", run.steps);
    assert!(
        fake.seen.borrow().is_empty(),
        "nothing was called: {:?}",
        fake.seen.borrow()
    );
    // notices keys on `ante.toml`, not on the kind, so a content repo with
    // one still gets that step, and it can turn the run red
    std::fs::write(d.path().join("ante.toml"), "").unwrap();
    for args in [
        &["-c", "user.name=t", "-c", "user.email=t@t", "add", "."][..],
        &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "two"][..],
    ] {
        let out = sh::run(d.path(), "git", args).unwrap();
        assert!(out.ok(), "{}", out.log());
    }
    let fake = Fake::new(vec![("ante check", 1, "missing header\n")]);
    let run = run_gate(&fake, d.path(), &m, "x", "t").unwrap();
    assert_eq!(run.verdict, Verdict::Red);
    assert_eq!(fake.seen.borrow().as_slice(), &["ante check"]);
    assert!(
        run.steps
            .iter()
            .filter(|s| !s.skipped)
            .all(|s| s.step == Step::Notices)
    );
}

#[test]
#[ignore = "catalogue: the gate lints the workspace at the root and never the engine workspace the root excludes under mock/; tracked the-gate-lints-the-engine-workspace-under-mock"]
fn a_root_that_excludes_a_workspace_under_mock_has_that_workspace_linted_too() {
    // The failure this catalogues: homma's root manifest is the launcher
    // workspace with `exclude = ["mock"]`, so the lint step's one clippy call
    // never reaches the engine, and thirteen findings sat there while the
    // gate stayed green on every tip. A second call, from the excluded
    // workspace's own root, is what would have caught them.
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"launcher\"]\nexclude = [\"mock\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.path().join("launcher/src")).unwrap();
    std::fs::write(
        d.path().join("launcher/Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.path().join("mock/crates/x-engine/src")).unwrap();
    std::fs::write(
        d.path().join("mock/Cargo.toml"),
        "[workspace]\nmembers = [\"crates/x-engine\"]\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("mock/crates/x-engine/Cargo.toml"),
        "[package]\nname = \"x-engine\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let fake = Fake::new(vec![]);
    run_step(&fake, d.path(), RepoKind::Crate, Step::Lint).unwrap();
    let clippy: Vec<String> = fake
        .seen
        .borrow()
        .iter()
        .filter(|l| l.starts_with("cargo clippy"))
        .cloned()
        .collect();
    assert_eq!(
        clippy.len(),
        2,
        "one clippy for the root and one for the workspace it excludes: {clippy:?}"
    );
}
