//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma repo config ...`: the shared tool configs a repo is meant to have.
//!
//! Two verbs. `check` compares and reports, writing nothing, and exits
//! non-zero when a repo is missing a config that is required of it. `init`
//! places what is missing.
//!
//! They are separate because the commit path needs the first and must not have
//! the second. Placing a config turns a check on, and a check that was not
//! running has not been passing, so its consequences land wherever that tool
//! looks, which is routinely somewhere nobody had in mind at the time. That is
//! somebody's to face deliberately, not something a gate does to them in the
//! middle of a commit.

use std::io::Write;
use std::path::Path;

use anyhow::{Result, anyhow};
use homma_core::Config;
use homma_org::configs;
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::{Outcome, util};
use crate::output::{HumanRender, emit};

/// What one repo owes, and what it has.
#[derive(Debug, Clone, Serialize)]
pub struct RepoConfigState {
    pub repo:       String,
    pub local_path: String,
    /// One line per finding, in the order the templates sort.
    pub findings:   Vec<String>,
    /// Whether anything here stops a commit.
    pub blocked:    bool,
}

impl RepoConfigState {
    /// Whether this repo has nothing worth printing in a summary.
    ///
    /// A config that matches is not news. Everything else is, at some
    /// severity, which is why the test is emptiness of the interesting set
    /// rather than absence of findings.
    pub fn is_quiet(&self) -> bool {
        self.findings.is_empty()
    }
}

/// The whole answer for a run.
#[derive(Debug, Serialize)]
pub struct ConfigReport {
    pub repos:      Vec<RepoConfigState>,
    /// Set when the shared configs could not be read at all.
    ///
    /// Carried rather than returned as an error: a workspace that has no
    /// shared configs yet is a real state, and saying so beats a stack trace.
    /// It does not block, because it is a fault in the workspace rather than
    /// in any repo, and refusing a repo's commits over it punishes the wrong
    /// party.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    pub blocked:    bool,
}

impl HumanRender for ConfigReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if let Some(why) = &self.unreadable {
            writeln!(out, "the shared configs could not be read: {why}")?;
            return Ok(());
        }
        for r in &self.repos {
            if r.is_quiet() {
                continue;
            }
            writeln!(out, "{}", r.repo)?;
            for f in &r.findings {
                writeln!(out, "  {f}")?;
            }
        }
        if self.blocked {
            writeln!(out)?;
            writeln!(out, "run `homma repo config init` to place what is missing")?;
        }
        Ok(())
    }
}

/// Compare every repo, or one, against the shared configs. Writes nothing.
pub fn check(cfg: &Config, repo: Option<&str>, format: OutputFormat) -> Result<Outcome> {
    let report = collect(cfg, repo)?;
    let blocked = report.blocked;
    emit(&report, format)?;
    Ok(if blocked { Outcome::ReportedFailure } else { Outcome::Ok })
}

/// Place what is missing, in every repo or one.
pub fn init(cfg: &Config, repo: Option<&str>, format: OutputFormat) -> Result<Outcome> {
    let ws = workspace_root(cfg)?;
    let templates = match configs::templates(ws.as_path()) {
        Ok(t) => t,
        Err(e) => {
            let report = ConfigReport {
                repos:      Vec::new(),
                unreadable: Some(e.to_string()),
                blocked:    false,
            };
            emit(&report, format)?;
            return Ok(Outcome::ReportedFailure);
        },
    };
    // The same list the regeneration writes under, rather than a second one
    // assembled here. Placing a config is a write into a member repo, which is
    // exactly what that list is drawn up to bound.
    let denied = crate::cmd::agent::denied_for_aggregating(cfg, &ws)?;
    let root = homma_api::Root::new(&ws, denied).map_err(|e| anyhow!("{e}"))?;

    let mut repos = Vec::new();
    let mut blocked = false;
    for (name, local) in members(cfg, repo)? {
        let findings = match contained(&root, &local) {
            Ok(c) => configs::ensure(&root, &c, &templates),
            Err(e) => {
                repos.push(RepoConfigState {
                    repo:       name,
                    local_path: local.display().to_string(),
                    findings:   vec![format!("not compared: {e}")],
                    blocked:    false,
                });
                continue;
            },
        };
        blocked |= findings.iter().any(configs::Finding::blocks);
        repos.push(RepoConfigState {
            repo:       name,
            local_path: local.display().to_string(),
            findings:   interesting(&findings),
            blocked:    false,
        });
    }
    let report = ConfigReport {
        repos,
        unreadable: None,
        blocked,
    };
    emit(&report, format)?;
    // Anything still blocking after a placement pass is something placement
    // could not fix, which is exactly what somebody needs to hear about.
    Ok(if blocked { Outcome::ReportedFailure } else { Outcome::Ok })
}

/// The read-only pass, shared with `homma status`.
pub fn collect(cfg: &Config, repo: Option<&str>) -> Result<ConfigReport> {
    let ws = workspace_root(cfg)?;
    let templates = match configs::templates(ws.as_path()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(ConfigReport {
                repos:      Vec::new(),
                unreadable: Some(e.to_string()),
                blocked:    false,
            });
        },
    };
    let mut repos = Vec::new();
    let mut blocked = false;
    for (name, local) in members(cfg, repo)? {
        let findings = configs::inspect(&local, &templates);
        let this_blocks = findings.iter().any(configs::Finding::blocks);
        blocked |= this_blocks;
        repos.push(RepoConfigState {
            repo:       name,
            local_path: local.display().to_string(),
            findings:   interesting(&findings),
            blocked:    this_blocks,
        });
    }
    Ok(ConfigReport {
        repos,
        unreadable: None,
        blocked,
    })
}

/// The findings worth printing.
///
/// A match is dropped, because a report listing every config that is fine is a
/// report nobody reads to the end, and the whole point of this being on the
/// commit path is that somebody reads it.
fn interesting(findings: &[configs::Finding]) -> Vec<String> {
    findings
        .iter()
        .filter(|f| !matches!(f, configs::Finding::Matches(_)))
        .map(ToString::to_string)
        .collect()
}

fn workspace_root(cfg: &Config) -> Result<homma_api::AbsPath> {
    let path =
        std::path::absolute(&cfg.workspace.path).unwrap_or_else(|_| cfg.workspace.path.clone());
    homma_api::AbsPath::new(path).map_err(|e| anyhow!("{e}"))
}

fn contained(
    root: &homma_api::Root,
    local: &Path,
) -> std::result::Result<homma_api::ContainedPath, String> {
    let abs = std::path::absolute(local).unwrap_or_else(|_| local.to_path_buf());
    homma_api::AbsPath::new(abs)
        .map_err(|e| e.to_string())
        .and_then(|a| root.contain(&a).map_err(|e| e.to_string()))
}

/// The members to walk, or the one named.
fn members(cfg: &Config, repo: Option<&str>) -> Result<Vec<(String, std::path::PathBuf)>> {
    let mut out = Vec::new();
    for (name, entry) in &cfg.repos {
        if let Some(filter) = repo {
            if filter != name {
                continue;
            }
        }
        out.push((
            name.clone(),
            util::resolve_local_path(&cfg.workspace.path, &entry.local_path),
        ));
    }
    if out.is_empty() {
        if let Some(name) = repo {
            return Err(util::no_such_member(cfg, name));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_config_is_not_worth_printing_and_everything_else_is() {
        // The summary exists to be read on a commit path, so what it drops
        // matters as much as what it keeps.
        let all = [
            configs::Finding::Matches("a".into()),
            configs::Finding::Differs("b".into()),
            configs::Finding::Missing("c".into(), configs::Severity::Required),
            configs::Finding::Missing("d".into(), configs::Severity::Suggested),
            configs::Finding::Placed("e".into()),
            configs::Finding::CannotInfer("f".into()),
            configs::Finding::NoVariantFits("g".into(), "why".into()),
            configs::Finding::Failed("h".into(), "why".into()),
        ];
        let kept = interesting(&all);
        assert_eq!(kept.len(), all.len() - 1, "exactly the match should drop");
        assert!(
            !kept.iter().any(|s| s.contains("a matches")),
            "a matching config reached the summary: {kept:?}"
        );
        // The control: a report of nothing but matches has nothing to say.
        assert!(interesting(&[configs::Finding::Matches("a".into())]).is_empty());
    }

    #[test]
    fn a_repo_with_nothing_interesting_is_quiet() {
        let quiet = RepoConfigState {
            repo:       "arvo".into(),
            local_path: "/ws/arvo".into(),
            findings:   Vec::new(),
            blocked:    false,
        };
        assert!(quiet.is_quiet());
        let loud = RepoConfigState {
            findings: vec!["deny.toml is missing, and is required here".into()],
            ..quiet.clone()
        };
        assert!(!loud.is_quiet());
    }

    #[test]
    fn an_unreadable_configs_directory_says_so_and_does_not_block() {
        // A workspace with no shared configs yet is a real state. Refusing
        // every commit in it would be this command's own gap stopping work in
        // repos that have done nothing wrong.
        let report = ConfigReport {
            repos:      Vec::new(),
            unreadable: Some("no shared configs at /ws/.shared/configs".into()),
            blocked:    false,
        };
        assert!(!report.blocked);
        let mut out = Vec::new();
        report.render_human(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("could not be read"), "{text}");
        assert!(
            !text.contains("homma repo config init"),
            "it told somebody to run a fix for a problem the fix cannot touch: {text}"
        );
    }

    #[test]
    fn a_blocked_report_names_the_command_that_fixes_it() {
        // The refusal is worth nothing if the thing it tells somebody to run
        // is not the thing that works. `cli.rs` pins that this parses.
        let report = ConfigReport {
            repos:      vec![RepoConfigState {
                repo:       "arvo".into(),
                local_path: "/ws/arvo".into(),
                findings:   vec!["deny.toml is missing, and is required here".into()],
                blocked:    true,
            }],
            unreadable: None,
            blocked:    true,
        };
        let mut out = Vec::new();
        report.render_human(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("homma repo config init"), "{text}");
        assert!(text.contains("arvo"), "{text}");
        assert!(text.contains("deny.toml"), "{text}");
    }

    #[test]
    fn a_clean_report_says_nothing_at_all() {
        // The control on the case above, and the shape nearly every run has.
        let report = ConfigReport {
            repos:      vec![RepoConfigState {
                repo:       "arvo".into(),
                local_path: "/ws/arvo".into(),
                findings:   Vec::new(),
                blocked:    false,
            }],
            unreadable: None,
            blocked:    false,
        };
        let mut out = Vec::new();
        report.render_human(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "");
    }
}
