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

use anyhow::{anyhow, Context, Result};
use homma_core::Config;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::util;
use crate::cmd::Outcome;
use crate::output::{emit, HumanRender};

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
        pub repo: String,
        pub local_path: String,
        pub state: AgentState,
        pub surfaces: Surfaces,
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
        pub mock_dir: bool,
        pub mock_agent_dir: bool,
        pub claude_dir: bool,
        pub github_instructions: bool,
        pub cargo_mock_alias: bool,
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
            mock_dir: local.join("mock").is_dir(),
            mock_agent_dir: local.join("mock/agent").is_dir(),
            claude_dir: local.join(".claude").is_dir(),
            github_instructions: local.join(".github/instructions").is_dir(),
            cargo_mock_alias: has_cargo_mock_alias(local),
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
        let core = s.mock_agent_dir
            && s.claude_dir
            && s.github_instructions
            && s.cargo_mock_alias;
        if core {
            AgentState::Configured
        } else {
            AgentState::Partial
        }
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
        if b {
            "yes"
        } else {
            "no"
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

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

    /// Roll-up regen report across all repos.
    #[derive(Debug, Serialize)]
    pub struct RegenReport {
        pub results: Vec<RegenResult>,
        pub ok: bool,
    }

    /// Per-repo regen outcome.
    #[derive(Debug, Serialize)]
    pub struct RegenResult {
        pub repo: String,
        pub status: RegenStatus,
        /// Last meaningful line from the subprocess output, when relevant.
        pub message: Option<String>,
    }

    #[derive(Debug, Clone, Copy, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RegenStatus {
        Success,
        /// `mock/` not present; no regen attempted. Not an error.
        Skipped,
        /// `cargo mock` returned non-zero.
        Failed,
    }

    pub fn run(
        cfg: &Config,
        repo: Option<&str>,
        continue_on_error: bool,
        format: OutputFormat,
    ) -> Result<Outcome> {
        if let Some(name) = repo {
            if cfg.repo(name).is_none() {
                return Err(anyhow!("repo `{name}` not declared in [repos.*]"));
            }
        }

        let mut results = Vec::new();
        let mut had_failure = false;
        for (name, repo_cfg) in &cfg.repos {
            if let Some(filter) = repo {
                if filter != name {
                    continue;
                }
            }
            let local = util::resolve_local_path(&cfg.workspace.path, &repo_cfg.local_path);
            let result = regen_one(name, &local);
            let failed = matches!(result.status, RegenStatus::Failed);
            results.push(result);
            if failed {
                had_failure = true;
                if !continue_on_error {
                    break;
                }
            }
        }

        let ok = !had_failure;
        emit(&RegenReport { results, ok }, format)?;
        Ok(if ok {
            Outcome::Ok
        } else {
            Outcome::ReportedFailure
        })
    }

    fn regen_one(name: &str, local: &Path) -> RegenResult {
        if !local.exists() {
            return RegenResult {
                repo: name.into(),
                status: RegenStatus::Failed,
                message: Some(format!("local_path {} does not exist", local.display())),
            };
        }
        if !local.join("mock").is_dir() {
            return RegenResult {
                repo: name.into(),
                status: RegenStatus::Skipped,
                message: Some("no mock/ directory".into()),
            };
        }
        match invoke_cargo_mock(local) {
            Ok(()) => RegenResult {
                repo: name.into(),
                status: RegenStatus::Success,
                message: None,
            },
            Err(e) => RegenResult {
                repo: name.into(),
                status: RegenStatus::Failed,
                message: Some(truncate(format!("{e:#}"), 256)),
            },
        }
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
                let tag = match r.status {
                    RegenStatus::Success => "ok",
                    RegenStatus::Skipped => "skip",
                    RegenStatus::Failed => "fail",
                };
                let msg = r.message.as_deref().unwrap_or("");
                if msg.is_empty() {
                    writeln!(out, "  [{tag}] {}", r.repo)?;
                } else {
                    writeln!(out, "  [{tag}] {}: {msg}", r.repo)?;
                }
            }
            Ok(())
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
