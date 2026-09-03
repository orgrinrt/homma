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
            GateError::Dirty => {
                write!(
                    f,
                    "the checkout has uncommitted changes; commit or stash first"
                )
            },
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

/// The gate on `sha`, which need not be the checkout's head: the head runs
/// in place, any other commit in a detached worktree made beside the
/// system's scratch for the run and removed after, so a push of a branch
/// that is not checked out is gated the same as one that is.
pub fn run_gate_at(
    runner: &dyn Runner,
    root: &Path,
    sha: &str,
    repo: &str,
    ran_at: &str,
) -> Result<GateRun, GateError> {
    if git::head(root)? == sha {
        return run_gate(runner, root, repo, ran_at);
    }
    let dir = std::env::temp_dir().join(format!(
        "homma-gate-{}-{}-{}",
        std::process::id(),
        &sha[.. 7.min(sha.len())],
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    git::worktree_add_detached(root, &dir, sha)?;
    let run = run_gate(runner, &dir, repo, ran_at);
    let removed = git::worktree_remove(root, &dir);
    let run = run?;
    removed?;
    Ok(run)
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
    // the wall time is the whole gate's; it rides on the last step that ran,
    // since a skipped step's numbers never reach the status line
    let wall = started.elapsed().as_secs_f64();
    if let Some(last) = steps.iter_mut().rev().find(|s| !s.skipped) {
        last.numbers
            .insert("wall_seconds".into(), format!("{wall:.1}"));
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
                outcome
                    .numbers
                    .insert("tests".into(), (t1 + t2).to_string());
                outcome
                    .numbers
                    .insert("passed".into(), (p1 + p2).to_string());
            }
        },
        Step::Docs => {
            if let Some(pct) = numbers::doc_coverage(&outcome.log) {
                outcome.numbers.insert("documented_percent".into(), pct);
            }
        },
        Step::Deny => {
            outcome.numbers.insert(
                "advisories".into(),
                numbers::deny_findings(&outcome.log).to_string(),
            );
        },
        _ => {},
    }
}

/// What a step runs on this repo, per the design's table. An empty list is a
/// step nothing asked for.
fn calls_for(
    root: &Path,
    repo_kind: RepoKind,
    step: Step,
) -> Result<Vec<Call<'static>>, GateError> {
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
        },
        Step::Lint => {
            if crate_ {
                let declared = feature_sets(root)?;
                calls.extend(cargo_runs(
                    &declared,
                    virtual_root(root)?,
                    &["clippy", "--all-targets"],
                    &["--", "-D", "warnings"],
                    false,
                ));
            }
            if deno {
                calls.push(Call::new("deno", &["lint"]));
                for export in deno_exports(root)? {
                    let mut c = Call::new("deno", &["check"]);
                    c.args.push(export);
                    calls.push(c);
                }
            }
        },
        Step::Tests => {
            if crate_ {
                let declared = feature_sets(root)?;
                calls.extend(cargo_runs(
                    &declared,
                    virtual_root(root)?,
                    &["test"],
                    &[],
                    true,
                ));
            }
            if deno {
                if deno_has_task(root, "test")? {
                    calls.push(Call::new("deno", &["task", "test"]));
                } else {
                    calls.push(Call::new("deno", &["test"]));
                }
            }
        },
        Step::Deny => {
            if crate_ && root.join("deny.toml").is_file() {
                calls.push(Call::new("cargo", &["deny", "check"]));
            }
        },
        Step::Docs => {
            if crate_ {
                let declared = feature_sets(root)?;
                calls.extend(
                    cargo_runs(
                        &declared,
                        virtual_root(root)?,
                        &["doc", "--no-deps"],
                        &[],
                        false,
                    )
                    .into_iter()
                    .map(|c| c.with_env("RUSTDOCFLAGS", "-Z unstable-options --show-coverage")),
                );
            }
            if deno {
                calls.push(Call::new("deno", &["doc", "--lint"]));
            }
        },
        Step::Notices => {
            if root.join("ante.toml").is_file() {
                calls.push(Call::new("ante", &["check"]));
            }
        },
    }
    Ok(calls)
}

/// The cargo calls one building step makes, shaped by what the tree declares.
/// `head` is the subcommand and its own flags, `tail` what follows `--`.
///
/// A root package declaring sets is built once per set and never with all
/// features, since a crate whose features exclude each other has said which
/// builds are legal. A root declaring none is built with all features, and
/// with none too where `with_none` asks for it; on a virtual root those runs
/// are workspace-wide and exclude every member that declares sets, while a
/// root that is itself a package builds alone as it always did. A member's
/// sets run against that member alone, `-p`.
fn cargo_runs(
    declared: &FeatureSets,
    virtual_root: bool,
    head: &[&str],
    tail: &[&str],
    with_none: bool,
) -> Vec<Call<'static>> {
    let mut calls = Vec::new();
    let tail: Vec<String> = tail.iter().map(|a| a.to_string()).collect();
    let with_set = |extra: &[String], set: &[String]| {
        let mut c = Call::new("cargo", head);
        c.args.extend(extra.iter().cloned());
        c.args.push("--no-default-features".into());
        if !set.is_empty() {
            c.args.push("--features".into());
            c.args.push(set.join(","));
        }
        c.args.extend(tail.iter().cloned());
        c
    };
    let members: Vec<&str> = declared.iter().filter_map(|(n, _)| n.as_deref()).collect();
    match declared.iter().find(|(n, _)| n.is_none()) {
        // FIXME: this arm reaches the root and no member. `members` is computed
        // above and never read here, so a root package declaring sets with
        // members under it builds only itself, where the arm below excludes
        // declaring members from a workspace-wide run and reaches the rest.
        // Whether a declaring root should speak for the whole tree is open;
        // `a_root_declaring_sets_still_builds_its_members` is the ignored arm
        // holding the case, and design round 202609030600 is the reasoning.
        Some((_, sets)) => {
            for set in sets {
                calls.push(with_set(&[], set));
            }
        },
        None => {
            let mut excludes: Vec<String> = Vec::new();
            if virtual_root && !members.is_empty() {
                excludes.push("--workspace".into());
                for m in &members {
                    excludes.push("--exclude".into());
                    excludes.push(m.to_string());
                }
            }
            let mut all = Call::new("cargo", head);
            all.args.push("--all-features".into());
            all.args.extend(excludes.iter().cloned());
            all.args.extend(tail.iter().cloned());
            calls.push(all);
            if with_none {
                let mut none = Call::new("cargo", head);
                none.args.push("--no-default-features".into());
                none.args.extend(excludes.iter().cloned());
                none.args.extend(tail.iter().cloned());
                calls.push(none);
            }
        },
    }
    for (name, sets) in declared.iter().filter(|(n, _)| n.is_some()) {
        let extra = ["-p".to_string(), name.clone().unwrap_or_default()];
        for set in sets {
            calls.push(with_set(&extra, set));
        }
    }
    calls
}

/// Every declared feature set with the member it belongs to: the root
/// package's with no name, each workspace member's with its package name,
/// so a member's sets run against that member and none is inherited.
pub type FeatureSets = Vec<(Option<String>, Vec<Vec<String>>)>;

/// Every `[package.metadata.homma] feature_sets` the tree declares; see
/// `FeatureSets`. Empty where nothing declares any.
pub fn feature_sets(root: &Path) -> Result<FeatureSets, GateError> {
    let mut found = Vec::new();
    let root_doc = manifest(&root.join("Cargo.toml"))?;
    if let Some(sets) = declared(&root_doc) {
        found.push((None, parse_sets(sets)?));
    }
    for (name, dir) in super::publish::crate_dirs(root) {
        if dir == root {
            continue;
        }
        let doc = manifest(&dir.join("Cargo.toml"))?;
        if let Some(sets) = declared(&doc) {
            found.push((Some(name), parse_sets(sets)?));
        }
    }
    Ok(found)
}

/// Whether the root manifest is a virtual workspace, one with no `[package]`
/// of its own, where a bare cargo run is already workspace-wide.
fn virtual_root(root: &Path) -> Result<bool, GateError> {
    Ok(manifest(&root.join("Cargo.toml"))?.get("package").is_none())
}

fn manifest(path: &Path) -> Result<toml::Value, GateError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| GateError::Manifest(format!("{}: {e}", path.display())))?;
    toml::from_str(&text).map_err(|e| GateError::Manifest(format!("{}: {e}", path.display())))
}

fn declared(doc: &toml::Value) -> Option<&Vec<toml::Value>> {
    doc.get("package")?
        .get("metadata")?
        .get("homma")?
        .get("feature_sets")?
        .as_array()
}

fn parse_sets(sets: &[toml::Value]) -> Result<Vec<Vec<String>>, GateError> {
    // an empty declaration would gate nothing and pass, so it is refused
    // rather than read as a crate with no builds
    if sets.is_empty() {
        return Err(GateError::Manifest(
            "feature_sets: an empty list declares no build; drop the key or name a set".into(),
        ));
    }
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
                    ));
                },
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
            map.values()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        },
        _ => Vec::new(),
    })
}

fn deno_has_task(root: &Path, task: &str) -> Result<bool, GateError> {
    let doc = deno_json(root)?;
    Ok(doc.get("tasks").and_then(|t| t.get(task)).is_some())
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
