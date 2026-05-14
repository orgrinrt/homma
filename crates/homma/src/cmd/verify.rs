//! `homma verify`: sanity-check `homma.toml` and the local environment.
//!
//! Checks performed:
//!
//! - Workspace root path exists (when configured to anything other than `.`).
//! - Each repo's `local_path` exists, resolved against the workspace root.
//! - Each repo references a forge that is declared in `[forges.*]`.
//! - Each forge `token_env`, when set, resolves to a non-empty value.
//!
//! Exits 0 with `ok: true` and an empty findings list when every check
//! passes, 1 with the failing findings otherwise. JSON mode emits the full
//! report regardless of pass/fail; the exit code is the contract for
//! pass/fail tooling consumers.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use homma_core::Config;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::{util, Outcome};
use crate::output::{emit, HumanRender};

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub level: Level,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Warn,
    Error,
}

pub fn run(cfg: &Config, format: OutputFormat) -> Result<Outcome> {
    let report = check(cfg);
    let ok = report.ok;
    emit(&report, format)?;
    Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
}

pub(crate) fn check(cfg: &Config) -> VerifyReport {
    let mut findings = Vec::new();

    let workspace_root = &cfg.workspace.path;
    if workspace_root != Path::new(".") && !workspace_root.exists() {
        findings.push(Finding {
            level: Level::Error,
            kind: "workspace_path_missing".into(),
            message: format!(
                "workspace.path = {} does not exist",
                workspace_root.display()
            ),
        });
    }

    for (name, repo) in &cfg.repos {
        if !cfg.forges.contains_key(&repo.forge) {
            findings.push(Finding {
                level: Level::Error,
                kind: "repo_forge_undeclared".into(),
                message: format!(
                    "repo `{name}` references forge `{}` which is not declared in [forges.*]",
                    repo.forge
                ),
            });
        }
        let local = util::resolve_local_path(workspace_root, &repo.local_path);
        if !local.exists() {
            findings.push(Finding {
                level: Level::Warn,
                kind: "repo_path_missing".into(),
                message: format!(
                    "repo `{name}` local_path = {} does not exist; `homma sync` will create it",
                    local.display()
                ),
            });
        }
    }

    for (name, forge) in &cfg.forges {
        if let Some(var) = forge.token_env.as_deref() {
            match std::env::var(var) {
                Ok(v) if !v.is_empty() => {}
                Ok(_) => findings.push(Finding {
                    level: Level::Warn,
                    kind: "forge_token_empty".into(),
                    message: format!(
                        "forge `{name}` token_env={var} is set but empty; mutating ops will fail unauthorized"
                    ),
                }),
                Err(_) => findings.push(Finding {
                    level: Level::Warn,
                    kind: "forge_token_unset".into(),
                    message: format!(
                        "forge `{name}` token_env={var} is not set in the environment; mutating ops will fail unauthorized"
                    ),
                }),
            }
        }
    }

    let ok = !findings.iter().any(|f| matches!(f.level, Level::Error));
    VerifyReport { ok, findings }
}

impl HumanRender for VerifyReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.ok && self.findings.is_empty() {
            writeln!(out, "OK")?;
            return Ok(());
        }
        writeln!(out, "{}", if self.ok { "OK with warnings" } else { "FAIL" })?;
        for f in &self.findings {
            let tag = match f.level {
                Level::Error => "error",
                Level::Warn => "warn",
            };
            writeln!(out, "  [{tag}] {}: {}", f.kind, f.message)?;
        }
        Ok(())
    }
}
