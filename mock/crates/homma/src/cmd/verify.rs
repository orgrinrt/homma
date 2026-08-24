//! `homma verify`: sanity-check `homma.toml` and the local environment.
//!
//! Checks performed:
//!
//! - Workspace root path exists (when configured to anything other than `.`).
//! - Each repo references a forge that is declared in `[forges.*]`.
//! - Each forge `token_env`, when set, resolves to a non-empty value.
//! - With `--forge`, each repo exists on its forge under the owner and name
//!   the manifest gives it. Off by default: it is a round-trip per repo where
//!   everything else is offline.
//!
//! A repo whose `local_path` is absent is **not** a finding. A workspace clones
//! the repos its work touches and leaves the rest, so most of the manifest is
//! absent from any given one and reporting that buries the findings that mean
//! something.
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
use crate::cmd::Outcome;
use crate::output::{HumanRender, emit};

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok:       bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub level:   Level,
    pub kind:    String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Warn,
    Error,
}

pub fn run(cfg: &Config, forge: bool, format: OutputFormat) -> Result<Outcome> {
    let mut report = check(cfg);
    if forge {
        report.findings.extend(resolve_on_their_forges(cfg));
        report.ok = !report.findings.iter().any(|f| matches!(f.level, Level::Error));
    }
    let ok = report.ok;
    emit(&report, format)?;
    Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
}

/// Ask each forge whether the repo is there under the owner and name the
/// manifest gives it.
///
/// This is the only check that sees a wrong `owner`. The field builds both the
/// API path and the clone URL, so a repo recorded under the wrong organisation
/// parses, verifies, and then 404s on the first forge operation, which may be
/// months later. Four entries in this workspace's own manifest sat wrong long
/// enough for the rules table to grow the same four errors, and neither caught
/// the other.
///
/// Separate from [`check`], and behind a flag, because that function is offline
/// and pure and is worth keeping so.
fn resolve_on_their_forges(cfg: &Config) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, forge_cfg) in &cfg.forges {
        if !has_a_token(forge_cfg) {
            findings.push(Finding {
                level:   Level::Warn,
                kind:    "forge_answers_are_not_evidence".into(),
                message: format!(
                    "forge `{name}` has no token in the environment, so a private repo there \
                     is indistinguishable from one that does not exist; not asking about its \
                     repos"
                ),
            });
        }
    }

    for (name, repo) in &cfg.repos {
        let Some(forge_cfg) = cfg.forges.get(&repo.forge) else {
            // Already reported by `check` as `repo_forge_undeclared`; saying it
            // twice in one report helps nobody.
            continue;
        };
        // **A negative answer from an unauthenticated forge means nothing.**
        // Every repo in this workspace is private, and GitHub answers 404 for a
        // private repo exactly as it does for an absent one. Reporting that as
        // a missing repo would have made this check fire on all twenty-four,
        // which is worse than not having it: a check that is wrong by default
        // is one people learn to skip.
        if !has_a_token(forge_cfg) {
            continue;
        }
        let client = crate::cmd::forge::client_from_config(forge_cfg);
        match client.repo_exists(&repo.owner, name) {
            Ok(true) => {},
            Ok(false) => {
                findings.push(Finding {
                    level:   Level::Error,
                    kind:    "repo_not_on_forge".into(),
                    message: format!(
                        "repo `{name}` is not at {}/{name} on forge `{}`; every forge \
                         operation against it will fail",
                        repo.owner, repo.forge
                    ),
                });
            },
            Err(e) => {
                // A network or token problem is not evidence about the repo, so
                // it is a warning rather than a verdict on the manifest.
                findings.push(Finding {
                    level:   Level::Warn,
                    kind:    "repo_forge_unreachable".into(),
                    message: format!(
                        "could not ask forge `{}` about {}/{name}: {e}",
                        repo.forge, repo.owner
                    ),
                });
            },
        }
    }
    findings
}

pub(crate) fn check(cfg: &Config) -> VerifyReport {
    let mut findings = Vec::new();

    let workspace_root = &cfg.workspace.path;
    if workspace_root != Path::new(".") && !workspace_root.exists() {
        findings.push(Finding {
            level:   Level::Error,
            kind:    "workspace_path_missing".into(),
            message: format!(
                "workspace.path = {} does not exist",
                workspace_root.display()
            ),
        });
    }

    for (name, repo) in &cfg.repos {
        if !cfg.forges.contains_key(&repo.forge) {
            findings.push(Finding {
                level:   Level::Error,
                kind:    "repo_forge_undeclared".into(),
                message: format!(
                    "repo `{name}` references forge `{}` which is not declared in [forges.*]",
                    repo.forge
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
    VerifyReport {
        ok,
        findings,
    }
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

/// Whether the forge's configured token is present and non-empty.
///
/// A forge that declares no `token_env` at all is treated the same as one whose
/// variable is unset: there is no credential either way.
fn has_a_token(forge: &homma_core::config::ForgeConfig) -> bool {
    forge
        .token_env
        .as_deref()
        .and_then(|var| std::env::var(var).ok())
        .is_some_and(|v| !v.is_empty())
}
