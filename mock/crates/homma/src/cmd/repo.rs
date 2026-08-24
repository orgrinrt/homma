//! `homma repo ...` commands.

use anyhow::Result;
use homma_core::Config;

pub mod status {
    use std::io::Write;

    use anyhow::{Context, anyhow};
    use homma_core::{GixRepo, RepoOps};
    use serde::Serialize;

    use super::*;
    use crate::cli::OutputFormat;
    use crate::cmd::util;
    use crate::output::{HumanRender, emit};

    #[derive(Debug, Serialize)]
    pub struct RepoStatusReport {
        pub repo:             String,
        pub local_path:       String,
        pub current_branch:   Option<String>,
        pub worktree_changes: usize,
        pub clean:            bool,
    }

    impl HumanRender for RepoStatusReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            writeln!(out, "repo: {}", self.repo)?;
            writeln!(out, "  path: {}", self.local_path)?;
            writeln!(
                out,
                "  branch: {}",
                self.current_branch.as_deref().unwrap_or("<detached>")
            )?;
            writeln!(
                out,
                "  worktree: {}",
                if self.clean {
                    "clean".to_string()
                } else {
                    format!("{} changes", self.worktree_changes)
                }
            )?;
            Ok(())
        }
    }

    pub fn run(cfg: &Config, repo_name: &str, format: OutputFormat) -> Result<()> {
        let entry = cfg
            .repo(repo_name)
            .ok_or_else(|| anyhow!("repo `{repo_name}` not declared in [repos.*]"))?;
        let local_path = util::resolve_local_path(&cfg.workspace.path, &entry.local_path);
        let repo = GixRepo::open(&local_path)
            .with_context(|| format!("opening repo at {}", local_path.display()))?;
        let status = repo.status().context("reading worktree status")?;
        let current_branch = repo.current_branch().context("reading current branch")?;
        let report = RepoStatusReport {
            repo: repo_name.into(),
            local_path: local_path.display().to_string(),
            current_branch,
            worktree_changes: status.worktree_changes,
            clean: status.is_clean,
        };
        emit(&report, format)?;
        Ok(())
    }
}
