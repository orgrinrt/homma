//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma agent ...`: workspace-level mockspace agent-template orchestration.
//!
//! Each member repo renders its own `.claude/` + `.github/instructions/` +
//! `.github/skills/` from `mock/agent/` templates via `cargo mock` (the
//! mockspace bootstrap-installed alias). `homma agent` is the workspace-
//! level orchestrator: it discovers per-repo state (`status`) or drives
//! the per-repo regen end-to-end (`regen`).
//!
//! The mockspace generator itself stays the source of truth for what each
//! repo's agent surface looks like; homma only sequences the invocation.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use homma_core::Config;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::{Outcome, util};
use crate::output::{HumanRender, emit};

pub mod status {
    use super::*;

    /// Per-repo agent-surface report.
    #[derive(Debug, Serialize)]
    pub struct AgentStatusReport {
        pub repos: Vec<RepoAgentState>,
    }

    /// Discovery result for one repo.
    #[derive(Debug, Serialize)]
    pub struct RepoAgentState {
        pub repo:       String,
        pub local_path: String,
        pub state:      AgentState,
        pub surfaces:   Surfaces,
    }

    /// Roll-up state.
    #[derive(Debug, Clone, Copy, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum AgentState {
        /// `mock/agent/` templates present and rendered into both `.claude/`
        /// and `.github/`, plus the `cargo mock` alias is configured.
        Configured,
        /// Some surfaces exist but not all. Worth running `homma agent regen`.
        Partial,
        /// No `mock/` directory at all. The repo has not adopted mockspace yet.
        NotConfigured,
        /// `local_path` does not exist on the filesystem.
        Missing,
    }

    /// Individual surface probes.
    #[derive(Debug, Serialize)]
    pub struct Surfaces {
        pub mock_dir:            bool,
        pub mock_agent_dir:      bool,
        pub claude_dir:          bool,
        pub github_instructions: bool,
        pub cargo_mock_alias:    bool,
    }

    pub fn run(cfg: &Config, repo: Option<&str>, format: OutputFormat) -> Result<()> {
        let report = AgentStatusReport {
            repos: collect(cfg, repo)?,
        };
        emit(&report, format)?;
        Ok(())
    }

    fn collect(cfg: &Config, repo: Option<&str>) -> Result<Vec<RepoAgentState>> {
        let mut out = Vec::new();
        for (name, repo_cfg) in &cfg.repos {
            if let Some(filter) = repo {
                if filter != name {
                    continue;
                }
            }
            let local = util::resolve_local_path(&cfg.workspace.path, &repo_cfg.local_path);
            out.push(probe(name, &local));
        }
        if out.is_empty() && repo.is_some() {
            return Err(anyhow!(
                "repo `{}` not declared in [repos.*]",
                repo.unwrap()
            ));
        }
        Ok(out)
    }

    fn probe(name: &str, local: &Path) -> RepoAgentState {
        let surfaces = Surfaces {
            mock_dir:            local.join("mock").is_dir(),
            mock_agent_dir:      local.join("mock/agent").is_dir(),
            claude_dir:          local.join(".claude").is_dir(),
            github_instructions: local.join(".github/instructions").is_dir(),
            cargo_mock_alias:    has_cargo_mock_alias(local),
        };
        let state = roll_up(local, &surfaces);
        RepoAgentState {
            repo: name.into(),
            local_path: local.display().to_string(),
            state,
            surfaces,
        }
    }

    fn roll_up(local: &Path, s: &Surfaces) -> AgentState {
        if !local.exists() {
            return AgentState::Missing;
        }
        if !s.mock_dir {
            return AgentState::NotConfigured;
        }
        let core = s.mock_agent_dir && s.claude_dir && s.github_instructions && s.cargo_mock_alias;
        if core { AgentState::Configured } else { AgentState::Partial }
    }

    /// Lightweight check: does `.cargo/config.toml` contain a `mock` alias?
    /// A real `[alias]` parse would be cleaner but adds a toml dep at this
    /// layer; substring is enough for discovery purposes.
    fn has_cargo_mock_alias(local: &Path) -> bool {
        let path = local.join(".cargo/config.toml");
        match std::fs::read_to_string(&path) {
            Ok(s) => s.contains("mock = ") || s.contains("mock=\""),
            Err(_) => false,
        }
    }

    impl HumanRender for AgentStatusReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            for r in &self.repos {
                let label = state_str(r.state);
                writeln!(out, "{}  [{label}]", r.repo)?;
                writeln!(out, "  path: {}", r.local_path)?;
                writeln!(
                    out,
                    "  mock={} mock/agent={} .claude={} .github/instructions={} cargo-mock-alias={}",
                    yn(r.surfaces.mock_dir),
                    yn(r.surfaces.mock_agent_dir),
                    yn(r.surfaces.claude_dir),
                    yn(r.surfaces.github_instructions),
                    yn(r.surfaces.cargo_mock_alias),
                )?;
            }
            Ok(())
        }
    }

    fn state_str(s: AgentState) -> &'static str {
        match s {
            AgentState::Configured => "configured",
            AgentState::Partial => "partial",
            AgentState::NotConfigured => "not configured",
            AgentState::Missing => "missing",
        }
    }

    fn yn(b: bool) -> &'static str {
        if b { "yes" } else { "no" }
    }

    #[cfg(test)]
    mod tests {
        use std::fs;

        use super::*;

        #[test]
        fn configured_when_all_surfaces_present() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(repo.join("mock/agent")).unwrap();
            fs::create_dir_all(repo.join(".claude")).unwrap();
            fs::create_dir_all(repo.join(".github/instructions")).unwrap();
            fs::create_dir_all(repo.join(".cargo")).unwrap();
            fs::write(
                repo.join(".cargo/config.toml"),
                b"[alias]\nmock = \"run\"\n",
            )
            .unwrap();
            let state = probe("r", &repo);
            assert!(matches!(state.state, AgentState::Configured));
        }

        #[test]
        fn partial_when_mock_present_but_render_missing() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(repo.join("mock/agent")).unwrap();
            let state = probe("r", &repo);
            assert!(matches!(state.state, AgentState::Partial));
        }

        #[test]
        fn not_configured_when_mock_absent() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(&repo).unwrap();
            let state = probe("r", &repo);
            assert!(matches!(state.state, AgentState::NotConfigured));
        }

        #[test]
        fn missing_when_local_path_absent() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("does-not-exist");
            let state = probe("r", &repo);
            assert!(matches!(state.state, AgentState::Missing));
        }

        #[test]
        fn cargo_alias_detected_via_substring() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(repo.join(".cargo")).unwrap();
            fs::write(
                repo.join(".cargo/config.toml"),
                b"[alias]\nmock = \"run --manifest-path ...\"\n",
            )
            .unwrap();
            assert!(has_cargo_mock_alias(&repo));
        }

        #[test]
        fn cargo_alias_absent_when_no_config() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(&repo).unwrap();
            assert!(!has_cargo_mock_alias(&repo));
        }
    }
}

pub mod regen {
    use super::*;
    use crate::cmd::aggregate;

    /// Roll-up regen report across all repos.
    #[derive(Debug, Serialize)]
    pub struct RegenReport {
        pub results:       Vec<RegenResult>,
        pub ok:            bool,
        /// Configs that differ from the shared copy, across the whole run.
        /// **A warning, never a failure**: a difference may be deliberate.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub diverged:      Vec<String>,
        /// Configs nothing could place, and why the stage could not run at all.
        /// These want somebody to act.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub needs_a_human: Vec<String>,
    }

    /// Per-repo regen outcome covering both pipeline stages.
    #[derive(Debug, Serialize)]
    pub struct RegenResult {
        pub repo:             String,
        pub cargo_mock:       StageStatus,
        pub configs:          Vec<String>,
        pub aggregate:        StageStatus,
        pub aggregated_hooks: usize,
    }

    /// Per-stage status, carrying a one-line reason or message when
    /// skipped or failed.
    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "snake_case", tag = "status", content = "detail")]
    pub enum StageStatus {
        Success,
        Skipped(String),
        Failed(String),
    }

    impl StageStatus {
        fn is_failure(&self) -> bool {
            matches!(self, StageStatus::Failed(_))
        }
    }

    /// Options for the regen pipeline.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Opts {
        pub continue_on_error: bool,
        pub skip_cargo_mock:   bool,
        pub skip_configs:      bool,
        pub skip_aggregate:    bool,
    }

    /// Full pipeline runner. Stage 1 runs `cargo mock` in each member
    /// repo; stage 2 aggregates per-repo rules and hooks into the
    /// workspace `.claude/`; stage 3 merges hook entries into
    /// workspace `settings.json`.
    pub fn run_with(
        cfg: &Config,
        repo: Option<&str>,
        opts: Opts,
        format: OutputFormat,
    ) -> Result<Outcome> {
        let report = regen(cfg, repo, opts)?;
        let ok = report.ok;
        emit(&report, format)?;
        Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
    }

    /// The pipeline itself, returning what it found rather than printing it.
    ///
    /// Split out from [`run_with`] so a test can assert on the per-repo results
    /// instead of parsing them back out of stdout.
    pub fn regen(cfg: &Config, repo: Option<&str>, opts: Opts) -> Result<RegenReport> {
        if let Some(name) = repo {
            if cfg.repo(name).is_none() {
                return Err(anyhow!("repo `{name}` not declared in [repos.*]"));
            }
        }
        // **Every stage, not two of them.** This guard was written when there
        // were two, and adding a third made it refuse a run that does real
        // work: comparing the shared configs is useful on its own, and is the
        // fast way to sweep a workspace without rebuilding anything.
        if opts.skip_cargo_mock && opts.skip_configs && opts.skip_aggregate {
            return Err(anyhow!(
                "`--skip-cargo-mock`, `--skip-configs` and `--skip-aggregate` together \
                 would do nothing"
            ));
        }

        let workspace = &cfg.workspace.path;

        // **A `Root` over the workspace, so every write below is proven against
        // the filesystem rather than checked as a prefix.** A previous round
        // checked `<workspace>/.claude` as one string while every path under it
        // was built with `Path::join`, which resolves nothing; a symlink one
        // component down carried the writes into the operator's own `.claude`,
        // deleting files there and installing executables, at exit 0.
        //
        // **The deny list is derived from the registry**, and no entry in it is
        // an absolute. What is forbidden is writing into somebody else's
        // workspace, so each one is denied to every participant for the same
        // reason and permitted to its owner, because nobody is denied their own.
        // Every workspace is a clone of the same shape, so regenerating one's
        // own is the ordinary path.
        //
        // The previous round refused the operator's own workspace by accident
        // through `Denied::from_env` and broke `agent regen` on the ordinary
        // configuration.
        let ws_abs = homma_api::AbsPath::new(
            std::path::absolute(workspace).unwrap_or_else(|_| workspace.clone()),
        )
        .map_err(|e| anyhow!("{e}"))?;
        let denied = denied_for_aggregating(cfg, &ws_abs)?;

        // **Refused here as well as per write, and the reason is the message
        // rather than the safety.** The `Root` below is what actually stops the
        // write, and it stops it correctly: nothing is created. But it fails
        // once per repository, inside the results table, where each line is
        // truncated, so an operator pointing homma at their own home saw several
        // clipped sentences instead of one reason.
        //
        // A denied aggregation target is a fact about the whole run, so it is
        // reported once, before any of it.
        denied
            .check(&ws_abs.join(".claude"), "workspace")
            .map_err(|e| anyhow!("{e}"))?;

        let root = homma_api::Root::new(&ws_abs, denied)
            .with_context(|| format!("aggregating into {}", ws_abs))?;

        // **Read once, before the loop.** The canonical configs are one
        // directory and every repo is compared against the same bytes; reading
        // them per repo would let a mid-run edit give two repos different
        // answers in one pass.
        //
        // Their absence is not a failure of the run. A workspace may not have
        // the directory yet, and `agent regen`'s other two stages are useful
        // without it, so the stage reports that it could not run and the rest
        // proceeds.
        let (templates, templates_err) = if opts.skip_configs {
            (Vec::new(), None)
        } else {
            match homma_org::configs::templates(&ws_abs) {
                Ok(t) => (t, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        };

        let mut settings_entries: Vec<aggregate::HookEntry> = Vec::new();
        // The repos whose hooks this run actually aggregated. A repo the
        // manifest declares but this workspace has not cloned is absent, and
        // its existing registrations are left alone rather than swept.
        let mut visited: Vec<String> = Vec::new();
        let mut results = Vec::new();
        let mut had_failure = false;
        let mut needs_a_human: Vec<String> = Vec::new();
        let mut diverged: Vec<String> = Vec::new();

        for (name, repo_cfg) in &cfg.repos {
            if let Some(filter) = repo {
                if filter != name {
                    continue;
                }
            }
            let local = util::resolve_local_path(workspace, &repo_cfg.local_path);

            // A member that is not on disk is not a case any more: membership
            // is detected by walking the tree, so a directory that is not
            // there is not a member and never reaches here. The branch that
            // reported it as skipped went with the manifest that produced it.
            // Kept as a guard rather than as a report, because a clone deleted
            // between the detection and this loop is possible and a missing
            // directory must not read as a failed regeneration.
            if !local.exists() {
                continue;
            }

            // Stage 1: cargo mock.
            let cargo_mock = if opts.skip_cargo_mock {
                StageStatus::Skipped("--skip-cargo-mock".into())
            } else if !local.join("mock").is_dir() {
                StageStatus::Skipped("no mock/ directory".into())
            } else {
                match invoke_cargo_mock(&local) {
                    Ok(()) => StageStatus::Success,
                    Err(e) => StageStatus::Failed(truncate(format!("{e:#}"), 256)),
                }
            };

            if cargo_mock.is_failure() {
                had_failure = true;
                results.push(RegenResult {
                    repo: name.clone(),
                    cargo_mock,
                    configs: Vec::new(),
                    aggregate: StageStatus::Skipped("cargo mock failed".into()),
                    aggregated_hooks: 0,
                });
                if !opts.continue_on_error {
                    break;
                }
                continue;
            }

            // Stage 2: the shared tool configs. A missing one whose home is
            // known is placed; one that differs is reported and left, because a
            // difference may be deliberate and nothing on disk says.
            let mut config_findings: Vec<String> = Vec::new();
            if !opts.skip_configs && !templates.is_empty() {
                match homma_api::AbsPath::new(
                    std::path::absolute(&local).unwrap_or_else(|_| local.clone()),
                )
                .map_err(|e| e.to_string())
                .and_then(|abs| root.contain(&abs).map_err(|e| e.to_string()))
                {
                    Ok(contained) => {
                        for f in homma_org::configs::ensure(&root, &contained, &templates) {
                            if f.needs_a_human() {
                                needs_a_human.push(format!("{name}: {f}"));
                            } else if matches!(f, homma_org::configs::Finding::Differs(_)) {
                                diverged.push(format!("{name}: {f}"));
                            }
                            config_findings.push(f.to_string());
                        }
                    },
                    // A repo the workspace root cannot contain is not one this
                    // stage may write into, and that is the containment
                    // mechanism working rather than a fault to report loudly.
                    Err(e) => config_findings.push(format!("not compared: {e}")),
                }
            }

            // Stage 3: aggregate. Only attempt if the repo has a
            // rendered .claude/ to read from.
            let claude_present = local.join(".claude").is_dir();
            let (aggregated_hooks, aggregate_stage) = if opts.skip_aggregate {
                (0, StageStatus::Skipped("--skip-aggregate".into()))
            } else if !claude_present {
                (0, StageStatus::Skipped("no .claude/ to aggregate".into()))
            } else {
                match aggregate::aggregate_repo(&root, name, &local, &mut settings_entries) {
                    Ok(h) => {
                        visited.push(name.clone());
                        (h, StageStatus::Success)
                    },
                    Err(e) => {
                        had_failure = true;
                        (0, StageStatus::Failed(truncate(format!("{e:#}"), 256)))
                    },
                }
            };

            let stage_failed = aggregate_stage.is_failure();
            results.push(RegenResult {
                repo: name.clone(),
                cargo_mock,
                configs: config_findings,
                aggregate: aggregate_stage,
                aggregated_hooks,
            });
            if stage_failed && !opts.continue_on_error {
                break;
            }
        }

        // Stage 3: write the workspace-level mockspace gate hook and
        // merge all entries (per-repo aggregated + workspace gate) into
        // settings.json.
        if !opts.skip_aggregate {
            let known_repos: Vec<&str> = cfg.repos.keys().map(String::as_str).collect();
            let visited_repos: Vec<&str> = visited.iter().map(String::as_str).collect();
            // The manifest's own `local_path`, workspace-relative, rather than
            // the absolute form this run resolved. The gate script is tracked,
            // so an absolute path in it names the workspace that generated it
            // and matches nothing anywhere else.
            let repo_paths: Vec<(String, String)> = cfg
                .repos
                .iter()
                .map(|(name, rc)| (name.clone(), rc.local_path.to_string_lossy().to_string()))
                .collect();
            let gate_entry = match crate::cmd::gates::install_workspace_gate(&root, &repo_paths) {
                Ok(e) => Some(e),
                Err(e) => {
                    had_failure = true;
                    results.push(RegenResult {
                        repo:             "(workspace gate)".into(),
                        cargo_mock:       StageStatus::Skipped("not a repo".into()),
                        configs:          Vec::new(),
                        aggregate:        StageStatus::Failed(truncate(format!("{e:#}"), 256)),
                        aggregated_hooks: 0,
                    });
                    None
                },
            };

            if let Err(e) = aggregate::merge_settings(
                &root,
                &known_repos,
                &visited_repos,
                &settings_entries,
                gate_entry.as_ref(),
            ) {
                had_failure = true;
                results.push(RegenResult {
                    repo:             "(settings.json)".into(),
                    cargo_mock:       StageStatus::Skipped("not a repo".into()),
                    configs:          Vec::new(),
                    aggregate:        StageStatus::Failed(truncate(format!("{e:#}"), 256)),
                    aggregated_hooks: 0,
                });
            }
        }

        if let Some(e) = templates_err {
            needs_a_human.push(format!("(configs): {e}"));
        }

        // **A divergence does not fail the run and a missing config does not
        // either.** Both are reported, and the exit status stays about whether
        // a stage failed to do its work. A tool that refuses to finish over a
        // workspace whose configs are merely unusual is a tool somebody starts
        // passing `--skip-configs` to, which loses the check entirely.
        let ok = !had_failure;
        Ok(RegenReport {
            results,
            ok,
            diverged,
            needs_a_human,
        })
    }

    /// Run `cargo mock` from the repo root. Errors carry the exit status +
    /// scrubbed stderr summary.
    fn invoke_cargo_mock(dir: &Path) -> Result<()> {
        let output = Command::new("cargo")
            .arg("mock")
            .current_dir(dir)
            .output()
            .with_context(|| format!("invoking `cargo mock` in {}", dir.display()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "`cargo mock` exited {}: {}",
                output.status,
                last_meaningful_line(&stderr).unwrap_or("(no stderr)"),
            ));
        }
        Ok(())
    }

    /// Pick the last non-empty trimmed line from a chunk of output. Used for
    /// summary messages, not full diagnostics; full output stays in stderr
    /// of the parent process if a user runs with `-vv`.
    fn last_meaningful_line(s: &str) -> Option<&str> {
        s.lines().rev().map(str::trim).find(|l| !l.is_empty())
    }

    fn truncate(mut s: String, max: usize) -> String {
        if s.len() <= max {
            return s;
        }
        let mut cut = max;
        while !s.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("...");
        s
    }

    impl HumanRender for RegenReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            writeln!(out, "regen: {}", if self.ok { "ok" } else { "FAIL" })?;
            for r in &self.results {
                let mock_tag = stage_tag(&r.cargo_mock);
                let agg_tag = stage_tag(&r.aggregate);
                writeln!(
                    out,
                    "  {}: cargo_mock={} aggregate={} (hooks={})",
                    r.repo, mock_tag, agg_tag, r.aggregated_hooks,
                )?;
                if let StageStatus::Failed(m) = &r.cargo_mock {
                    writeln!(out, "    cargo_mock: {m}")?;
                }
                if let StageStatus::Failed(m) = &r.aggregate {
                    writeln!(out, "    aggregate: {m}")?;
                }
                for c in &r.configs {
                    writeln!(out, "    configs: {c}")?;
                }
            }
            // Repeated below the table, because a per-repo line scrolls past
            // and the whole point of the stage is the handful of lines an
            // operator has to do something about.
            if !self.diverged.is_empty() {
                writeln!(
                    out,
                    "\nconfigs that differ from the shared copy (left as they are):"
                )?;
                for d in &self.diverged {
                    writeln!(out, "  {d}")?;
                }
            }
            if !self.needs_a_human.is_empty() {
                writeln!(out, "\nconfigs somebody has to place:")?;
                for d in &self.needs_a_human {
                    writeln!(out, "  {d}")?;
                }
            }
            Ok(())
        }
    }

    fn stage_tag(s: &StageStatus) -> &'static str {
        match s {
            StageStatus::Success => "ok",
            StageStatus::Skipped(_) => "skip",
            StageStatus::Failed(_) => "fail",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn last_meaningful_line_picks_last_nonempty() {
            let s = "first\nsecond\n\n   \nthird\n";
            assert_eq!(last_meaningful_line(s), Some("third"));
        }

        #[test]
        fn last_meaningful_line_empty_input_is_none() {
            assert_eq!(last_meaningful_line(""), None);
            assert_eq!(last_meaningful_line("\n\n   \n"), None);
        }

        #[test]
        fn truncate_short_is_unchanged() {
            assert_eq!(truncate("hello".into(), 16), "hello");
        }

        #[test]
        fn truncate_long_clipped_with_ellipsis() {
            let s = "abcdefghij".to_string();
            let out = truncate(s, 5);
            assert_eq!(out, "abcde...");
        }

        #[test]
        fn truncate_walks_back_to_char_boundary() {
            // Two-byte chars; cutting at 3 lands mid-char.
            let s = "ééé".to_string(); // 6 bytes
            let out = truncate(s, 3);
            // Walks back to byte 2 (between first and second char).
            assert!(out.ends_with("..."));
            assert!(out.is_char_boundary(out.len() - 3));
        }
    }
}

// Tests for `status::probe` live inside the `status` module (see above).

/// The places this pass may not aggregate into.
///
/// A home's own `.claude`, which is never a workspace, plus **every
/// participant's workspace except the one being written into**, which is the
/// actor's by definition. That last exclusion is what keeps the list correct
/// rather than paralysing: a workspace is one participant's, denied to every
/// other and permitted to its owner.
///
/// The registry is optional in this configuration, and its absence means the
/// list is the home-derived pair alone. That is a real state rather than a gap:
/// a workspace with no participants has no participant workspaces to protect.
fn denied_for_aggregating(
    cfg: &Config,
    workspace: &homma_api::AbsPath,
) -> Result<homma_api::Denied> {
    // The manifest's own `deny` may name the workspace being aggregated into,
    // which is a thing an operator can reasonably write and which would then
    // refuse the aggregation it was asked for. So the permission the registry
    // loop below performs has to cover that list too, and it does because
    // `permitting` runs after everything is folded in rather than before.
    //
    // The workspace root is passed as the base and decides nothing. A config
    // read from a file has had its relative entries anchored to the manifest's
    // own directory already, which is the anchor `denying` documents, so nothing
    // relative is left for a base to disagree about. The two coincide in the
    // ordinary layout and part the moment `workspace.path` points elsewhere,
    // which is a manifest the parser accepts.
    let mut denied = homma_api::Denied::from_env()?
        .denying(
            &cfg.deny,
            workspace,
            homma_api::Denied::home().ok().as_ref(),
        )
        .permitting(workspace);
    let Some(org) = cfg.org.as_ref() else {
        return Ok(denied);
    };
    let Ok(ws) = org
        .clone()
        .try_into::<std::collections::BTreeMap<String, homma_api::Identity>>()
    else {
        return Ok(denied);
    };
    for id in ws.values() {
        let Some(w) = id.workspace.as_ref() else {
            continue;
        };
        let theirs = homma_api::AbsPath::resolve(workspace, w);
        if theirs == *workspace {
            continue;
        }
        denied = denied.and(theirs, "it is another participant's workspace");
    }
    Ok(denied)
}
