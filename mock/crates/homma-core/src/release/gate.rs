//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The gate: six steps run against one clean checkout, each producing a pass
//! or a fail, the numbers it measured and everything it printed. What each
//! step runs per repo kind is the table in `DEEPDIVE_release.md`.

use std::fmt;
use std::path::Path;
use std::time::Instant;

use homma_api::{GateRun, RepoKind, Step, StepOutcome};

use super::{git, kind, numbers, sh};

/// How the gate reaches a program. The real one spawns it; a test hands the
/// gate what a tool would have printed.
pub trait Runner {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn>;
}

/// The runner that spawns the programs.
pub struct Real;

impl Runner for Real {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        sh::run_with_env(cwd, program, args, env)
    }
}

/// Why the gate did not run at all, as opposed to running and going red.
#[derive(Debug)]
pub enum GateError {
    /// Unstaged or uncommitted changes: a number measured there describes no
    /// commit.
    Dirty,
    NoManifest(kind::NoManifest),
    Git(git::GitError),
    Spawn(sh::Spawn),
    /// The manifest could not be read or parsed.
    Manifest(String),
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateError::Dirty => write!(f, "the checkout has uncommitted changes; commit or stash first"),
            GateError::NoManifest(e) => write!(f, "{e}"),
            GateError::Git(e) => write!(f, "{e}"),
            GateError::Spawn(e) => write!(f, "{e}"),
            GateError::Manifest(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for GateError {}

impl From<git::GitError> for GateError {
    fn from(e: git::GitError) -> Self {
        GateError::Git(e)
    }
}

impl From<sh::Spawn> for GateError {
    fn from(e: sh::Spawn) -> Self {
        GateError::Spawn(e)
    }
}

/// Run the whole gate on `root`, recording it as `repo` at `ran_at`. Refuses
/// a dirty tree before running anything.
pub fn run_gate(
    runner: &dyn Runner,
    root: &Path,
    repo: &str,
    ran_at: &str,
) -> Result<GateRun, GateError> {
    if !git::is_clean(root)? {
        return Err(GateError::Dirty);
    }
    let sha = git::head(root)?;
    let repo_kind = kind::detect(root).map_err(GateError::NoManifest)?;
    let started = Instant::now();
    let mut steps = Vec::with_capacity(Step::ALL.len());
    for step in Step::ALL {
        steps.push(run_step(runner, root, repo_kind, step)?);
    }
    let wall = started.elapsed().as_secs_f64();
    if let Some(last) = steps.last_mut() {
        last.numbers.insert("wall_seconds".into(), format!("{wall:.1}"));
    }
    let verdict = GateRun::verdict_of(&steps);
    Ok(GateRun {
        repo: repo.into(),
        sha,
        ran_at: ran_at.into(),
        verdict,
        steps,
    })
}

/// One invocation inside a step: the arguments, and the environment for the
/// call alone.
struct Call<'a> {
    program: &'a str,
    args:    Vec<String>,
    env:     Vec<(&'a str, &'a str)>,
}

impl<'a> Call<'a> {
    fn new(program: &'a str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|a| a.to_string()).collect(),
            env: Vec::new(),
        }
    }

    fn with_env(mut self, key: &'a str, value: &'a str) -> Self {
        self.env.push((key, value));
        self
    }
}

/// Run one step: every call the kind asks for, in order, stopping at the first
/// that fails so a red log ends on the failure.
pub fn run_step(
    runner: &dyn Runner,
    root: &Path,
    repo_kind: RepoKind,
    step: Step,
) -> Result<StepOutcome, GateError> {
    let calls = calls_for(root, repo_kind, step)?;
    if calls.is_empty() {
        return Ok(StepOutcome::skipped(step));
    }
    let mut outcome = StepOutcome {
        step,
        passed: true,
        skipped: false,
        numbers: Default::default(),
        log: String::new(),
    };
    for call in &calls {
        let args: Vec<&str> = call.args.iter().map(String::as_str).collect();
        let out = runner.run(root, call.program, &args, &call.env)?;
        outcome.log.push_str("$ ");
        outcome.log.push_str(&out.command_line());
        outcome.log.push('\n');
        outcome.log.push_str(&out.log());
        if !outcome.log.ends_with('\n') {
            outcome.log.push('\n');
        }
        if !out.ok() {
            outcome.passed = false;
            break;
        }
    }
    measure(&mut outcome);
    Ok(outcome)
}

/// The numbers a step's log carries, read once the log is complete.
fn measure(outcome: &mut StepOutcome) {
    match outcome.step {
        Step::Tests => {
            let cargo = numbers::cargo_tests(&outcome.log);
            let deno = numbers::deno_tests(&outcome.log);
            if cargo.is_some() || deno.is_some() {
                let (t1, p1) = cargo.unwrap_or((0, 0));
                let (t2, p2) = deno.unwrap_or((0, 0));
                outcome.numbers.insert("tests".into(), (t1 + t2).to_string());
                outcome.numbers.insert("passed".into(), (p1 + p2).to_string());
            }
        }
        Step::Docs => {
            if let Some(pct) = numbers::doc_coverage(&outcome.log) {
                outcome.numbers.insert("documented_percent".into(), pct);
            }
        }
        Step::Deny => {
            outcome
                .numbers
                .insert("advisories".into(), numbers::deny_findings(&outcome.log).to_string());
        }
        _ => {}
    }
}

/// What a step runs on this repo, per the design's table. An empty list is a
/// step nothing asked for.
fn calls_for(root: &Path, repo_kind: RepoKind, step: Step) -> Result<Vec<Call<'static>>, GateError> {
    let mut calls = Vec::new();
    let crate_ = repo_kind.has_crate();
    let deno = repo_kind.has_deno();
    match step {
        Step::Format => {
            if crate_ {
                calls.push(Call::new("cargo", &["fmt", "--check"]));
            }
            if deno {
                calls.push(Call::new("deno", &["fmt", "--check"]));
            }
        }
        Step::Lint => {
            if crate_ {
                calls.push(Call::new("cargo", &[
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ]));
            }
            if deno {
                calls.push(Call::new("deno", &["lint"]));
                for export in deno_exports(root)? {
                    let mut c = Call::new("deno", &["check"]);
                    c.args.push(export);
                    calls.push(c);
                }
            }
        }
        Step::Tests => {
            if crate_ {
                calls.push(Call::new("cargo", &["test", "--all-features"]));
                let sets = feature_sets(root)?;
                if sets.is_empty() {
                    calls.push(Call::new("cargo", &["test", "--no-default-features"]));
                } else {
                    for set in sets {
                        let mut c = Call::new("cargo", &["test", "--no-default-features"]);
                        if !set.is_empty() {
                            c.args.push("--features".into());
                            c.args.push(set.join(","));
                        }
                        calls.push(c);
                    }
                }
            }
            if deno {
                if deno_has_task(root, "test")? {
                    calls.push(Call::new("deno", &["task", "test"]));
                } else {
                    calls.push(Call::new("deno", &["test"]));
                }
            }
        }
        Step::Deny => {
            if crate_ && root.join("deny.toml").is_file() {
                calls.push(Call::new("cargo", &["deny", "check"]));
            }
        }
        Step::Docs => {
            if crate_ {
                calls.push(
                    Call::new("cargo", &["doc", "--no-deps", "--all-features"])
                        .with_env("RUSTDOCFLAGS", "-Z unstable-options --show-coverage"),
                );
            }
            if deno {
                calls.push(Call::new("deno", &["doc", "--lint"]));
            }
        }
        Step::Notices => {
            if root.join("ante.toml").is_file() {
                calls.push(Call::new("ante", &["check"]));
            }
        }
    }
    Ok(calls)
}

/// `[package.metadata.homma] feature_sets` off the root manifest, or off
/// the first workspace member that declares one; empty where none does.
pub fn feature_sets(root: &Path) -> Result<Vec<Vec<String>>, GateError> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|e| GateError::Manifest(format!("Cargo.toml: {e}")))?;
    let doc: toml::Value =
        toml::from_str(&text).map_err(|e| GateError::Manifest(format!("Cargo.toml: {e}")))?;
    let sets = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("homma"))
        .and_then(|h| h.get("feature_sets"))
        .and_then(|s| s.as_array());
    let Some(sets) = sets else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for set in sets {
        let Some(items) = set.as_array() else {
            return Err(GateError::Manifest(
                "feature_sets: each set is a list of feature names".into(),
            ));
        };
        let mut names = Vec::new();
        for item in items {
            match item.as_str() {
                Some(s) => names.push(s.to_string()),
                None => {
                    return Err(GateError::Manifest(
                        "feature_sets: a feature name is a string".into(),
                    ))
                }
            }
        }
        out.push(names);
    }
    Ok(out)
}

fn deno_json(root: &Path) -> Result<serde_json::Value, GateError> {
    let text = std::fs::read_to_string(root.join("deno.json"))
        .map_err(|e| GateError::Manifest(format!("deno.json: {e}")))?;
    serde_json::from_str(&text).map_err(|e| GateError::Manifest(format!("deno.json: {e}")))
}

/// Every export path `deno.json` declares, whether `exports` is one string
/// or a map.
pub fn deno_exports(root: &Path) -> Result<Vec<String>, GateError> {
    let doc = deno_json(root)?;
    Ok(match doc.get("exports") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Object(map)) => {
            map.values().filter_map(|v| v.as_str().map(str::to_string)).collect()
        }
        _ => Vec::new(),
    })
}

fn deno_has_task(root: &Path, task: &str) -> Result<bool, GateError> {
    let doc = deno_json(root)?;
    Ok(doc.get("tasks").and_then(|t| t.get(task)).is_some())
}

#[cfg(test)]
mod tests {
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
                .find(|(prefix, _, _)| line.starts_with(prefix))
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
        let fake = Fake::new(vec![("cargo test", 0, "test result: ok. 2 passed; 0 failed; 0 ignored\n")]);
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
    fn feature_sets_from_the_manifest_each_get_a_run() {
        let d = crate_root(
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [[], [\"a\"], [\"a\", \"b\"]]\n",
        );
        assert_eq!(feature_sets(d.path()).unwrap(), vec![
            vec![],
            vec!["a".to_string()],
            vec!["a".to_string(), "b".to_string()]
        ]);
        let fake = Fake::new(vec![]);
        run_step(&fake, d.path(), RepoKind::Crate, Step::Tests).unwrap();
        assert_eq!(fake.seen.borrow().as_slice(), &[
            "cargo test --all-features",
            "cargo test --no-default-features",
            "cargo test --no-default-features --features a",
            "cargo test --no-default-features --features a,b"
        ]);
    }

    #[test]
    fn a_malformed_feature_set_is_a_manifest_error_not_a_skip() {
        let d = crate_root(
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.homma]\nfeature_sets = [\"a\"]\n",
        );
        assert!(matches!(feature_sets(d.path()), Err(GateError::Manifest(_))));
    }

    #[test]
    fn a_failing_call_stops_the_step_and_turns_a_blocking_step_red() {
        let d = crate_root("[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
        let fake = Fake::new(vec![("cargo test --all-features", 101, "test result: FAILED. 1 passed; 1 failed; 0 ignored\n")]);
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
        assert!(run_step(&fake, d.path(), RepoKind::Crate, Step::Deny).unwrap().skipped);
        assert!(run_step(&fake, d.path(), RepoKind::Crate, Step::Notices).unwrap().skipped);
        std::fs::write(d.path().join("deny.toml"), "").unwrap();
        std::fs::write(d.path().join("ante.toml"), "").unwrap();
        let fake = Fake::new(vec![("cargo deny", 1, "error[vulnerability]: x\n")]);
        let deny = run_step(&fake, d.path(), RepoKind::Crate, Step::Deny).unwrap();
        assert!(deny.is_red());
        assert_eq!(deny.numbers["advisories"], "1");
        let notices = run_step(&fake, d.path(), RepoKind::Crate, Step::Notices).unwrap();
        assert!(notices.passed && !notices.skipped);
        assert_eq!(fake.seen.borrow().as_slice(), &["cargo deny check", "ante check"]);
    }

    #[test]
    fn a_deno_package_lints_then_checks_every_export_and_tests_through_its_task() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("deno.json"),
            r#"{"exports": {".": "./mod.ts", "./x": "./x.ts"}, "tasks": {"test": "deno test -A"}}"#,
        )
        .unwrap();
        let fake = Fake::new(vec![("deno task test", 0, "ok | 3 passed | 0 failed (1ms)\n")]);
        let lint = run_step(&fake, d.path(), RepoKind::Deno, Step::Lint).unwrap();
        assert!(lint.passed);
        let tests = run_step(&fake, d.path(), RepoKind::Deno, Step::Tests).unwrap();
        assert_eq!(tests.numbers["tests"], "3");
        let seen = fake.seen.borrow();
        assert_eq!(seen[0], "deno lint");
        assert!(seen[1..3].contains(&"deno check ./mod.ts".to_string()));
        assert!(seen[1..3].contains(&"deno check ./x.ts".to_string()));
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
        std::fs::write(d.path().join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.1.0\"\n").unwrap();
        std::fs::write(d.path().join("deno.json"), "{}").unwrap();
        let fake = Fake::new(vec![]);
        run_step(&fake, d.path(), RepoKind::Both, Step::Format).unwrap();
        assert_eq!(fake.seen.borrow().as_slice(), &["cargo fmt --check", "deno fmt --check"]);
    }

    #[test]
    fn the_whole_gate_refuses_a_dirty_tree_and_runs_every_step_on_a_clean_one() {
        let d = git_repo_with(&[("Cargo.toml", "[package]\nname=\"x\"\nversion=\"0.1.0\"\n")]);
        let fake = Fake::new(vec![("cargo test", 0, "test result: ok. 1 passed; 0 failed; 0 ignored\n")]);
        let run = run_gate(&fake, d.path(), "x", "2026-09-02T20:00:00Z").unwrap();
        assert_eq!(run.verdict, Verdict::Green);
        assert_eq!(run.steps.len(), Step::ALL.len());
        assert_eq!(run.sha, git::head(d.path()).unwrap());
        assert!(run.steps.last().unwrap().numbers.contains_key("wall_seconds"));
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
        assert_eq!(run_gate(&fake, d.path(), "x", "t").unwrap().verdict, Verdict::Green);
        let fake = Fake::new(vec![("cargo fmt", 1, "Diff in src/lib.rs\n")]);
        let run = run_gate(&fake, d.path(), "x", "t").unwrap();
        assert_eq!(run.verdict, Verdict::Red);
        assert!(run.steps[0].log.contains("Diff in src/lib.rs"));
    }

    #[test]
    fn a_tree_with_no_manifest_is_an_error_not_an_empty_green() {
        let d = git_repo_with(&[("README.md", "hi")]);
        let fake = Fake::new(vec![]);
        assert!(matches!(run_gate(&fake, d.path(), "x", "t"), Err(GateError::NoManifest(_))));
    }
}
