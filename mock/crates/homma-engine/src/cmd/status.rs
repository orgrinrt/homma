//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma status`: what state the workspace is in.
//!
//! Covers every population homma can report without touching the network: the
//! manifest and its forges, the agent surfaces including whether each repo's
//! git hooks are wired, the shared tool configs, and the working tree.
//!
//! By default it prints only what is not in the state it should be in. That is
//! what the question is usually asking, and it is what keeps the answer
//! readable across a workspace of thirty members. `--full` prints every
//! population whole.
//!
//! The narrower verbs stay the place to see one population entirely, healthy
//! entries included. What this buys is not having to know which of them to
//! reach for.
//!
//! The doc surfaces are deliberately not here. `homma docs status` reports what
//! each repo ships, and a repo missing a `CHANGELOG.md` is not in a state that
//! stops anything: it is a survey rather than a fault.

use std::io::Write;

use anyhow::Result;
use homma_core::local::{Local, LocalError};
use homma_core::{Config, ForgeKind, Injected};
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::agent::status::RepoAgentState;
use crate::cmd::config::ConfigReport;
use crate::cmd::stray_targets::{self, StrayTarget};
use crate::cmd::{Outcome, config, util};
use crate::output::{HumanRender, emit};

/// Top-level success payload for `homma status`.
///
/// Fields are added rather than restructured as populations join, so anything
/// reading this document keeps working and simply ignores what it does not
/// know.
#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub workspace:      WorkspaceLine,
    /// What this clone is for, out of `homma.local.toml`. `null` where the
    /// clone has no such file, or has one that does not load.
    pub instance:       Option<homma_core::local::Instance>,
    /// Why `homma.local.toml` did not load, where it is there and does not.
    /// The one place a broken file is reported; the loader reads only `repos` from it and widens the scope.
    pub instance_error: Option<String>,
    /// Where the instance's values came from, where the file names a project: the
    /// project, and the dotted paths inherited, set here, overridden and unset.
    pub project:        Option<homma_core::inherit::Provenance>,
    /// The stored config a clone holds a full copy of, with no project line, so
    /// a change to the store will not reach it. `homma local install` migrates.
    pub holds_copy_of:  Option<String>,
    /// Repositories the instance's scope names that no clone provides. Not an error: a
    /// container that has not attached one yet is ordinary. Empty with no scope.
    pub not_cloned:     Vec<String>,
    pub forges:         Vec<ForgeLine>,
    pub repos:          Vec<RepoLine>,
    /// What the workspace's own tools said, in the order `[[status.inject]]`
    /// declares them. Empty where the manifest declares none, which is most of
    /// them.
    pub injected:       Vec<Injected>,
    /// The agent surfaces, per repo, including the git hooks wiring.
    pub agent:          Vec<RepoAgentState>,
    /// The shared tool configs, per repo.
    pub configs:        ConfigReport,
    /// The working tree, per repo.
    pub worktrees:      Vec<WorktreeLine>,
    /// Build directories other than the workspace's one `target/`, with their
    /// sizes. Empty is the healthy state, and it is the one thing here that
    /// is walked from the disk rather than read from a repository.
    pub stray_targets:  Vec<StrayTarget>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceLine {
    pub name:                   String,
    pub path:                   String,
    pub default_public_branch:  String,
    pub default_working_branch: String,
}

#[derive(Debug, Serialize)]
pub struct ForgeLine {
    pub name:      String,
    pub kind:      String,
    pub base_url:  String,
    pub api_url:   String,
    pub token_env: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RepoLine {
    pub name:           String,
    /// Absent where the clone's remote names no configured forge, which is a
    /// real state rather than a gap: a member with a local remote has none.
    pub forge:          Option<String>,
    /// Absent on the same terms.
    pub owner:          Option<String>,
    pub local_path:     String,
    pub public_branch:  String,
    pub working_branch: String,
}

/// One repo's working tree.
#[derive(Debug, Serialize)]
pub struct WorktreeLine {
    pub repo:    String,
    /// `None` where the path is not a repository, or is not there at all.
    pub branch:  Option<String>,
    pub clean:   bool,
    pub changes: usize,
    /// Set where the tree could not be read, which is a state rather than a
    /// failure of the command: a member the workspace declares and nobody has
    /// cloned reads exactly like this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread:  Option<String>,
}

impl WorktreeLine {
    /// Whether this has nothing worth printing in a summary.
    fn is_quiet(&self) -> bool {
        self.clean && self.unread.is_none()
    }
}

pub fn run(
    cfg: &Config,
    local: Result<Option<Local>, LocalError>,
    full: bool,
    format: OutputFormat,
) -> Result<Outcome> {
    // Every population is gathered here rather than inside `build_report`, so
    // that stays a pure function of what it is handed and can be tested without
    // spawning or touching a disk.
    let injected = homma_core::inject::run_all(&cfg.status, &cfg.workspace.path);
    let agent = crate::cmd::agent::status::collect(cfg, None)?;
    let configs = config::collect(cfg, None)?;
    let worktrees = worktrees(cfg);
    let mut report = build_report(cfg, injected, agent, configs, worktrees);
    report.stray_targets = stray_targets::collect(&cfg.workspace.path, &cfg.scope);
    report.carry_local(local);
    report.note_copy(&cfg.workspace.path);
    // The human rendering summarises and the document does not, so the flag
    // reaches only the first. A machine handed a document with the healthy
    // members dropped would be reading a lie about the population, and it has
    // no way to notice, where a person reading a terminal does.
    match format {
        OutputFormat::Human => {
            let mut out = std::io::stdout().lock();
            report.render(&mut out, full)?;
        },
        OutputFormat::Json => emit(&report, format)?,
    }
    Ok(Outcome::Ok)
}

/// The working tree of every member.
fn worktrees(cfg: &Config) -> Vec<WorktreeLine> {
    use homma_core::{GixRepo, RepoOps};

    cfg.repos
        .iter()
        .map(|(name, r)| {
            let local = util::resolve_local_path(&cfg.workspace.path, &r.local_path);
            let read = GixRepo::open(&local)
                .map_err(|e| e.to_string())
                .and_then(|repo| {
                    let status = repo.status().map_err(|e| e.to_string())?;
                    let branch = repo.current_branch().map_err(|e| e.to_string())?;
                    Ok((status, branch))
                });
            match read {
                Ok((status, branch)) => {
                    WorktreeLine {
                        repo: name.clone(),
                        branch,
                        clean: status.is_clean,
                        changes: status.worktree_changes,
                        unread: None,
                    }
                },
                Err(e) => {
                    WorktreeLine {
                        repo:    name.clone(),
                        branch:  None,
                        clean:   true,
                        changes: 0,
                        unread:  Some(e),
                    }
                },
            }
        })
        .collect()
}

fn build_report(
    cfg: &Config,
    injected: Vec<Injected>,
    agent: Vec<RepoAgentState>,
    configs: ConfigReport,
    worktrees: Vec<WorktreeLine>,
) -> StatusReport {
    let workspace = WorkspaceLine {
        name:                   cfg.workspace.name.clone(),
        path:                   cfg.workspace.path.display().to_string(),
        default_public_branch:  cfg.defaults.public_branch.clone(),
        default_working_branch: cfg.defaults.working_branch.clone(),
    };
    let forges = cfg
        .forges
        .iter()
        .map(|(name, f)| {
            ForgeLine {
                name:      name.clone(),
                kind:      forge_kind_str(f.kind).to_string(),
                base_url:  f.base_url.clone(),
                api_url:   f.api_url.clone(),
                token_env: f.token_env.clone(),
            }
        })
        .collect();
    let repos = cfg
        .repos
        .iter()
        .map(|(name, r)| {
            RepoLine {
                name:           name.clone(),
                forge:          r.forge.clone(),
                owner:          r.owner.clone(),
                local_path:     r.local_path.display().to_string(),
                public_branch:  cfg.defaults.public_branch.clone(),
                working_branch: cfg.defaults.working_branch.clone(),
            }
        })
        .collect();
    StatusReport {
        workspace,
        instance: None,
        instance_error: None,
        project: None,
        holds_copy_of: None,
        not_cloned: cfg
            .scope
            .unmet(cfg.repos.keys().map(String::as_str))
            .into_iter()
            .map(str::to_owned)
            .collect(),
        forges,
        repos,
        injected,
        agent,
        configs,
        worktrees,
        stray_targets: Vec::new(),
    }
}

fn forge_kind_str(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::Github => "github",
        ForgeKind::Forgejo => "forgejo",
    }
}

impl StatusReport {
    /// Carry what reading `homma.local.toml` gave: the instance, or why not.
    fn carry_local(&mut self, local: Result<Option<Local>, LocalError>) {
        match local {
            Ok(l) => {
                self.project = l.as_ref().and_then(|l| l.provenance.clone());
                self.instance = l.map(|l| l.instance);
            },
            Err(e) => {
                // The whole chain, since the parse error under the file's name
                // is what says which line to fix.
                let mut msg = e.to_string();
                let mut src = std::error::Error::source(&e);
                while let Some(s) = src {
                    msg.push_str(": ");
                    msg.push_str(s.to_string().trim_end());
                    src = s.source();
                }
                self.instance_error = Some(msg);
            },
        }
    }

    /// Note a clone that holds a full copy of the stored config of its work,
    /// which `workspace` is the root of.
    fn note_copy(&mut self, workspace: &std::path::Path) {
        let Some(i) = &self.instance else { return };
        if i.project.is_none()
            && workspace
                .join(homma_core::local::STORE_DIR)
                .join(format!("{}.toml", i.work))
                .is_file()
        {
            self.holds_copy_of = Some(i.work.clone());
        }
    }

    /// Render for a person. `full` prints every population whole.
    pub fn render(&self, out: &mut dyn Write, full: bool) -> std::io::Result<()> {
        writeln!(
            out,
            "workspace: {} ({})",
            self.workspace.name, self.workspace.path
        )?;
        writeln!(
            out,
            "  default public={}  working={}",
            self.workspace.default_public_branch, self.workspace.default_working_branch
        )?;
        if let Some(i) = &self.instance {
            match &i.title {
                Some(title) => writeln!(out, "  this clone is for {} ({title})", i.work)?,
                None => writeln!(out, "  this clone is for {}", i.work)?,
            }
        }
        if let Some(p) = &self.project {
            writeln!(
                out,
                "  inherits {}: {} inherited, {} set here, {} overridden, {} unset",
                p.project,
                p.inherited.len(),
                p.local.len(),
                p.overridden.len(),
                p.unset.len()
            )?;
        }
        if let Some(w) = &self.holds_copy_of {
            writeln!(
                out,
                "  holds a copy of the stored {w}; `homma local install {w}` makes it inherit"
            )?;
        }
        if let Some(e) = &self.instance_error {
            writeln!(out, "  what this clone is for is unknown: {e}")?;
        }
        if !self.not_cloned.is_empty() {
            writeln!(
                out,
                "  in scope and not cloned: {}",
                self.not_cloned.join(", ")
            )?;
        }
        if !self.forges.is_empty() && full {
            writeln!(out)?;
            writeln!(out, "forges:")?;
            for f in &self.forges {
                let tok = f.token_env.as_deref().unwrap_or("<none>");
                writeln!(
                    out,
                    "  {} [{}] {} (api={}, token_env={})",
                    f.name, f.kind, f.base_url, f.api_url, tok
                )?;
            }
        }
        if !self.repos.is_empty() && full {
            writeln!(out)?;
            writeln!(out, "repos:")?;
            for r in &self.repos {
                // A member with no forge prints as one rather than as a blank
                // where a name should be, so the line says which state it is
                // in instead of leaving the reader to guess at an empty field.
                let on = match (&r.owner, &r.forge) {
                    (Some(owner), Some(forge)) => format!("{owner}/{} on {forge}", r.name),
                    (Some(owner), None) => format!("{owner}/{}, forge unknown", r.name),
                    (None, _) => format!("{}, no forge remote", r.name),
                };
                writeln!(
                    out,
                    "  {} -> {on} (path={}, public={}, working={})",
                    r.name, r.local_path, r.public_branch, r.working_branch
                )?;
            }
        }

        self.render_agent(out, full)?;
        self.render_configs(out, full)?;
        self.render_worktrees(out, full)?;
        self.render_stray_targets(out)?;

        for block in &self.injected {
            // A tool with nothing to say prints no heading: a clone whose config
            // turns a tool off (the dashboard, where a console replaces it) shows
            // no line for it, rather than an empty one.
            if block.failed.is_none() && block.text.trim().is_empty() {
                continue;
            }
            writeln!(out)?;
            match &block.failed {
                // The failure goes on the heading rather than under it, so a
                // block with nothing in it does not read as a tool that had
                // nothing to say.
                Some(why) => writeln!(out, "{}: {why}", block.title)?,
                None => {
                    writeln!(out, "{}:", block.title)?;
                    // Indented here rather than by the tool, which does not
                    // know it is being embedded and prints the same either way
                    // when run by hand. A blank line stays blank; two spaces on
                    // it would be trailing whitespace in somebody's terminal.
                    for line in block.text.lines() {
                        if line.is_empty() {
                            writeln!(out)?;
                        } else {
                            writeln!(out, "  {line}")?;
                        }
                    }
                },
            }
        }
        Ok(())
    }

    fn render_agent(&self, out: &mut dyn Write, full: bool) -> std::io::Result<()> {
        use crate::cmd::agent::status::AgentState;

        // A repo with no mockspace is not short of anything, so it is listed
        // under `--full` as what it is and never in the summary.
        let show: Vec<&RepoAgentState> = self
            .agent
            .iter()
            .filter(|a| {
                match a.state {
                    AgentState::Configured | AgentState::NotConfigured => full,
                    AgentState::Partial | AgentState::Missing => true,
                }
            })
            .collect();
        if show.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "agent surfaces:")?;
        let mut short = false;
        for a in show {
            if matches!(a.state, AgentState::Missing) {
                writeln!(out, "  {} not on disk", a.repo)?;
                continue;
            }
            if matches!(a.state, AgentState::NotConfigured) {
                writeln!(out, "  {} no mockspace", a.repo)?;
                continue;
            }
            let missing = missing_surfaces(a);
            if missing.is_empty() {
                writeln!(out, "  {} ok", a.repo)?;
            } else {
                short = true;
                writeln!(out, "  {} missing {}", a.repo, missing.join(", "))?;
            }
        }
        if short {
            writeln!(out, "  run `homma agent regen` to render what is missing")?;
        }
        Ok(())
    }

    fn render_configs(&self, out: &mut dyn Write, full: bool) -> std::io::Result<()> {
        if let Some(why) = &self.configs.unreadable {
            writeln!(out)?;
            writeln!(out, "shared configs: could not be read: {why}")?;
            return Ok(());
        }
        let show: Vec<_> = self
            .configs
            .repos
            .iter()
            .filter(|r| full || !r.is_quiet())
            .collect();
        if show.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "shared configs:")?;
        for r in show {
            if r.findings.is_empty() {
                writeln!(out, "  {} ok", r.repo)?;
                continue;
            }
            writeln!(out, "  {}", r.repo)?;
            for f in &r.findings {
                writeln!(out, "    {f}")?;
            }
        }
        if self.configs.blocked {
            writeln!(
                out,
                "  run `homma repo config init` to place what is missing"
            )?;
        }
        Ok(())
    }

    fn render_stray_targets(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.stray_targets.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(
            out,
            "build directories other than the workspace's one target/:"
        )?;
        for s in &self.stray_targets {
            writeln!(out, "  {}  {}", stray_targets::human(s.bytes), s.path)?;
        }
        Ok(())
    }

    fn render_worktrees(&self, out: &mut dyn Write, full: bool) -> std::io::Result<()> {
        let show: Vec<&WorktreeLine> = self
            .worktrees
            .iter()
            .filter(|w| full || !w.is_quiet())
            .collect();
        if show.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "working trees:")?;
        for w in show {
            match (&w.unread, &w.branch) {
                (Some(e), _) => writeln!(out, "  {} not read: {e}", w.repo)?,
                (None, branch) => {
                    let on = branch.as_deref().unwrap_or("<detached>");
                    if w.clean {
                        writeln!(out, "  {} clean on {on}", w.repo)?;
                    } else {
                        writeln!(out, "  {} {} changes on {on}", w.repo, w.changes)?;
                    }
                },
            }
        }
        Ok(())
    }
}

/// Which agent surfaces an adopted repo does not have.
fn missing_surfaces(a: &RepoAgentState) -> Vec<String> {
    let s = &a.surfaces;
    let mut out = Vec::new();
    for (present, name) in [
        (s.mock_dir, format!("{}/", s.mock_dir_name)),
        (s.claude_dir, ".claude/".to_string()),
        (s.github_instructions, ".github/instructions/".to_string()),
        (s.git_hooks_path, "git-hooks".to_string()),
    ] {
        if !present {
            out.push(name);
        }
    }
    out
}

impl HumanRender for StatusReport {
    /// The summarised form, which is what every caller through `emit` wants.
    ///
    /// `run` bypasses this for the human path so `--full` can reach the
    /// renderer; the trait has nowhere to carry a flag.
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        self.render(out, false)
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
