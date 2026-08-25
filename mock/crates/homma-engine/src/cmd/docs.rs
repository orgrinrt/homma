//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma docs ...`: workspace-level documentation discovery.
//!
//! Phase 1 ships only `status`: per-repo surface probe reporting which of
//! `README.md`, `docs/`, `mock/DESIGN.md.tmpl`, `mock/PRINCIPLES.md.tmpl`,
//! `mock/WORKFLOW.md.tmpl`, and `CHANGELOG.md` each member repo ships.
//!
//! Workspace-level aggregation / render lands in a follow-up round.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, anyhow};
use homma_core::Config;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::util;
use crate::output::{HumanRender, emit};

pub mod status {
    use super::*;

    /// Per-repo doc-surface report.
    #[derive(Debug, Serialize)]
    pub struct DocsStatusReport {
        pub repos: Vec<RepoDocsState>,
    }

    /// Discovery result for one repo.
    #[derive(Debug, Serialize)]
    pub struct RepoDocsState {
        pub repo:              String,
        pub local_path:        String,
        pub local_path_exists: bool,
        pub surfaces:          DocSurfaces,
    }

    /// Individual surface probes.
    #[derive(Debug, Serialize)]
    pub struct DocSurfaces {
        pub readme:               bool,
        pub docs_dir:             bool,
        pub mock_design_tmpl:     bool,
        pub mock_principles_tmpl: bool,
        pub mock_workflow_tmpl:   bool,
        pub changelog:            bool,
    }

    pub fn run(cfg: &Config, repo: Option<&str>, format: OutputFormat) -> Result<()> {
        let report = DocsStatusReport {
            repos: collect(cfg, repo)?,
        };
        emit(&report, format)?;
        Ok(())
    }

    fn collect(cfg: &Config, repo: Option<&str>) -> Result<Vec<RepoDocsState>> {
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

    fn probe(name: &str, local: &Path) -> RepoDocsState {
        let surfaces = DocSurfaces {
            readme:               local.join("README.md").is_file(),
            docs_dir:             local.join("docs").is_dir(),
            mock_design_tmpl:     local.join("mock/DESIGN.md.tmpl").is_file(),
            mock_principles_tmpl: local.join("mock/PRINCIPLES.md.tmpl").is_file(),
            mock_workflow_tmpl:   local.join("mock/WORKFLOW.md.tmpl").is_file(),
            changelog:            local.join("CHANGELOG.md").is_file(),
        };
        RepoDocsState {
            repo: name.into(),
            local_path: local.display().to_string(),
            local_path_exists: local.exists(),
            surfaces,
        }
    }

    impl HumanRender for DocsStatusReport {
        fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
            for r in &self.repos {
                writeln!(out, "{}", r.repo)?;
                writeln!(out, "  path: {}", r.local_path)?;
                if !r.local_path_exists {
                    writeln!(out, "  [missing on filesystem]")?;
                    continue;
                }
                writeln!(
                    out,
                    "  README={} docs/={} CHANGELOG={}",
                    yn(r.surfaces.readme),
                    yn(r.surfaces.docs_dir),
                    yn(r.surfaces.changelog),
                )?;
                writeln!(
                    out,
                    "  mock/: DESIGN.md.tmpl={} PRINCIPLES.md.tmpl={} WORKFLOW.md.tmpl={}",
                    yn(r.surfaces.mock_design_tmpl),
                    yn(r.surfaces.mock_principles_tmpl),
                    yn(r.surfaces.mock_workflow_tmpl),
                )?;
            }
            Ok(())
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
        fn probe_reports_all_surfaces_present() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(repo.join("docs")).unwrap();
            fs::create_dir_all(repo.join("mock")).unwrap();
            for f in [
                "README.md",
                "CHANGELOG.md",
                "mock/DESIGN.md.tmpl",
                "mock/PRINCIPLES.md.tmpl",
                "mock/WORKFLOW.md.tmpl",
            ] {
                fs::write(repo.join(f), b"").unwrap();
            }
            let state = probe("r", &repo);
            assert!(state.local_path_exists);
            assert!(state.surfaces.readme);
            assert!(state.surfaces.docs_dir);
            assert!(state.surfaces.changelog);
            assert!(state.surfaces.mock_design_tmpl);
            assert!(state.surfaces.mock_principles_tmpl);
            assert!(state.surfaces.mock_workflow_tmpl);
        }

        #[test]
        fn probe_reports_missing_path() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("does-not-exist");
            let state = probe("r", &repo);
            assert!(!state.local_path_exists);
            assert!(!state.surfaces.readme);
        }

        #[test]
        fn probe_partial_surfaces() {
            let dir = tempfile::tempdir().unwrap();
            let repo = dir.path().join("r");
            fs::create_dir_all(&repo).unwrap();
            fs::write(repo.join("README.md"), b"").unwrap();
            let state = probe("r", &repo);
            assert!(state.local_path_exists);
            assert!(state.surfaces.readme);
            assert!(!state.surfaces.docs_dir);
            assert!(!state.surfaces.mock_design_tmpl);
        }
    }
}
