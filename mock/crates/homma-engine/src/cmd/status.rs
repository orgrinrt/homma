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
    /// The one place a broken file is reported, since nothing else reads it.
    pub instance_error: Option<String>,
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
    report.carry_local(local);
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
        forges,
        repos,
        injected,
        agent,
        configs,
        worktrees,
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
            Ok(l) => self.instance = l.map(|l| l.instance),
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
        if let Some(e) = &self.instance_error {
            writeln!(out, "  what this clone is for is unknown: {e}")?;
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

        for block in &self.injected {
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

        let show: Vec<&RepoAgentState> = self
            .agent
            .iter()
            .filter(|a| full || !matches!(a.state, AgentState::Configured))
            .collect();
        if show.is_empty() {
            return Ok(());
        }
        writeln!(out)?;
        writeln!(out, "agent surfaces:")?;
        for a in show {
            let missing = missing_surfaces(a);
            if missing.is_empty() {
                writeln!(out, "  {} ok", a.repo)?;
            } else {
                writeln!(out, "  {} missing {}", a.repo, missing.join(", "))?;
            }
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

/// Which agent surfaces a repo does not have.
fn missing_surfaces(a: &RepoAgentState) -> Vec<&'static str> {
    let s = &a.surfaces;
    let mut out = Vec::new();
    for (present, name) in [
        (s.mock_dir, "mock/"),
        (s.mock_agent_dir, "mock/agent/"),
        (s.claude_dir, ".claude/"),
        (s.github_instructions, ".github/instructions/"),
        (s.cargo_mock_alias, "cargo-mock-alias"),
        (s.git_hooks_path, "git-hooks"),
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
mod tests {
    use super::*;
    use crate::cmd::agent::status::{AgentState, Surfaces};
    use crate::cmd::config::RepoConfigState;

    fn block(title: &str, text: &str, failed: Option<&str>) -> Injected {
        Injected {
            title:  title.into(),
            text:   text.into(),
            failed: failed.map(str::to_string),
        }
    }

    fn empty_configs() -> ConfigReport {
        ConfigReport {
            repos:      Vec::new(),
            unreadable: None,
            blocked:    false,
        }
    }

    fn report(injected: Vec<Injected>) -> StatusReport {
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        build_report(&cfg, injected, Vec::new(), empty_configs(), Vec::new())
    }

    fn with(
        agent: Vec<RepoAgentState>,
        configs: ConfigReport,
        worktrees: Vec<WorktreeLine>,
    ) -> StatusReport {
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        build_report(&cfg, Vec::new(), agent, configs, worktrees)
    }

    fn surfaces(all: bool) -> Surfaces {
        Surfaces {
            mock_dir:            true,
            mock_agent_dir:      true,
            claude_dir:          true,
            github_instructions: true,
            cargo_mock_alias:    true,
            git_hooks_path:      all,
        }
    }

    fn agent_state(repo: &str, state: AgentState, all: bool) -> RepoAgentState {
        RepoAgentState {
            repo: repo.into(),
            local_path: format!("/ws/{repo}"),
            state,
            surfaces: surfaces(all),
        }
    }

    fn rendered(report: &StatusReport) -> String {
        let mut out = Vec::new();
        report
            .render_human(&mut out)
            .expect("writing to a vec cannot fail");
        String::from_utf8(out).expect("the render is utf-8")
    }

    fn rendered_full(report: &StatusReport) -> String {
        let mut out = Vec::new();
        report
            .render(&mut out, true)
            .expect("writing to a vec cannot fail");
        String::from_utf8(out).expect("the render is utf-8")
    }

    /// The report over a real file read from a real directory, the way `run`
    /// is handed it.
    fn with_local(local: &str) -> StatusReport {
        let d = tempfile::tempdir().expect("a temp dir");
        std::fs::write(d.path().join(homma_core::local::LOCAL_FILE), local).expect("writable");
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        let mut r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
        r.carry_local(Local::load(d.path()));
        r
    }

    #[test]
    fn a_broken_local_file_is_reported_rather_than_refused_or_ignored() {
        // A missing key, bad syntax, and an unknown field. Each leaves the
        // instance null, names the file, and carries the parser's own detail.
        for (broken, detail) in [
            ("[instance]\n", "work"),
            ("[instance\n", "line 1"),
            ("[instance]\nwork = \"k\"\nwhat = 1\n", "what"),
        ] {
            let r = with_local(broken);
            assert!(r.instance.is_none(), "{broken:?}");
            let e = r.instance_error.as_deref().expect("the error is carried");
            assert!(e.contains(homma_core::local::LOCAL_FILE), "{e}");
            assert!(e.contains(detail), "{broken:?} lost its detail: {e}");
            let human = rendered(&r);
            assert!(
                human.contains("what this clone is for is unknown: "),
                "{human}"
            );
            let doc = serde_json::to_value(&r).unwrap();
            assert!(doc["instance"].is_null(), "{doc}");
            assert_eq!(doc["instance_error"], e);
        }
    }

    #[test]
    fn a_good_or_absent_local_file_carries_no_error() {
        let r = with_local("[instance]\nwork = \"k\"\n");
        assert!(r.instance_error.is_none());
        assert!(!rendered(&r).contains("unknown"));
        assert!(serde_json::to_value(&r).unwrap()["instance_error"].is_null());
        let r = report(Vec::new());
        assert!(r.instance_error.is_none());
    }

    #[test]
    fn a_clone_without_a_local_file_says_nothing_about_what_it_is_for() {
        let r = report(Vec::new());
        assert!(r.instance.is_none());
        assert!(!rendered(&r).contains("this clone is for"));
        let doc = serde_json::to_value(&r).unwrap();
        assert!(doc["instance"].is_null(), "{doc}");
    }

    #[test]
    fn a_clone_with_a_local_file_says_what_it_is_for() {
        let r = with_local("[instance]\nwork = \"kenno\"\n");
        assert!(
            rendered(&r).contains("  this clone is for kenno\n"),
            "{}",
            rendered(&r)
        );
        let r = with_local("[instance]\nwork = \"kenno\"\ntitle = \"the tile machine\"\n");
        assert!(
            rendered(&r).contains("  this clone is for kenno (the tile machine)\n"),
            "{}",
            rendered(&r)
        );
    }

    #[test]
    fn the_document_carries_the_instance_whole() {
        let r = with_local(
            "[instance]\nwork = \"k\"\nrepos = [\"a\"]\nstate = \"s.md\"\ngoal = \"g.md\"\n[tools.x]\ny = \"z\"\n",
        );
        let doc = serde_json::to_value(&r).unwrap();
        assert_eq!(doc["instance"]["work"], "k");
        assert_eq!(doc["instance"]["repos"][0], "a");
        assert_eq!(doc["instance"]["state"], "s.md");
        assert_eq!(doc["instance"]["goal"], "g.md");
        // The tools' tables are theirs and stay out of the status document.
        assert!(
            doc.get("tools").is_none() && doc["instance"].get("tools").is_none(),
            "{doc}"
        );
    }

    #[test]
    fn a_block_prints_its_title_and_its_text_indented() {
        let text = rendered(&report(vec![block("context", "450733 of 1000000", None)]));
        assert!(text.contains("context:\n  450733 of 1000000\n"), "{text}");
    }

    #[test]
    fn a_blank_line_inside_a_block_stays_blank() {
        // Two spaces on an otherwise empty line is trailing whitespace in
        // somebody's terminal and in every diff of a captured status.
        let text = rendered(&report(vec![block("t", "a\n\nb", None)]));
        assert!(text.contains("  a\n\n  b\n"), "{text:?}");
    }

    #[test]
    fn a_failed_block_says_so_on_the_heading() {
        // On the heading rather than under it, so a block with nothing in it
        // does not read as a tool that had nothing to say.
        let text = rendered(&report(vec![block("agenda", "", Some("agenda exited 1"))]));
        assert!(text.contains("agenda: agenda exited 1\n"), "{text}");
        assert!(
            !text.contains("agenda:\n"),
            "the empty-body shape must not appear: {text}"
        );
    }

    #[test]
    fn blocks_print_in_the_order_they_arrived() {
        let text = rendered(&report(vec![
            block("first", "1", None),
            block("second", "2", None),
            block("third", "3", None),
        ]));
        let at = |needle: &str| {
            text.find(needle)
                .unwrap_or_else(|| panic!("missing {needle} in {text}"))
        };
        assert!(at("first:") < at("second:"));
        assert!(at("second:") < at("third:"));
    }

    #[test]
    fn a_report_with_no_injections_prints_nothing_extra() {
        // The control. Every assertion above would hold against a render that
        // printed blocks unconditionally, and a workspace declaring none is
        // every workspace but one.
        let bare = rendered(&report(vec![]));
        let with = rendered(&report(vec![block("t", "x", None)]));
        assert!(!bare.contains("t:"), "{bare}");
        assert_eq!(with.len() - bare.len(), "\nt:\n  x\n".len(), "{with:?}");
    }

    #[test]
    fn the_blocks_reach_the_json_document_too() {
        let doc = serde_json::to_value(report(vec![block("context", "45%", None)]))
            .expect("the report serialises");
        let blocks = doc["injected"].as_array().expect("an injected array");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["title"], "context");
        assert_eq!(blocks[0]["text"], "45%");
        assert!(
            blocks[0].get("failed").is_none(),
            "a block that worked carries no failure key: {blocks:?}"
        );
    }

    #[test]
    fn a_failed_block_carries_its_reason_into_the_json_too() {
        let doc = serde_json::to_value(report(vec![block("agenda", "", Some("exited 1"))]))
            .expect("the report serialises");
        assert_eq!(doc["injected"][0]["failed"], "exited 1");
    }

    #[test]
    fn building_a_report_spawns_nothing() {
        // `build_report` is a pure function of what it is handed, which is what
        // lets it be tested at all. Every population is gathered in `run`.
        let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
        let r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
        assert!(r.injected.is_empty());
        assert!(r.agent.is_empty());
        assert!(r.worktrees.is_empty());
    }

    #[test]
    fn a_healthy_workspace_says_nothing_about_any_population() {
        // The whole default. Thirty healthy members must not produce thirty
        // blocks, or nobody reads the one line that matters.
        let text = rendered(&with(
            vec![agent_state("arvo", AgentState::Configured, true)],
            ConfigReport {
                repos:      vec![RepoConfigState {
                    repo:       "arvo".into(),
                    local_path: "/ws/arvo".into(),
                    findings:   Vec::new(),
                    blocked:    false,
                }],
                unreadable: None,
                blocked:    false,
            },
            vec![WorktreeLine {
                repo:    "arvo".into(),
                branch:  Some("dev".into()),
                clean:   true,
                changes: 0,
                unread:  None,
            }],
        ));
        assert!(!text.contains("agent surfaces:"), "{text}");
        assert!(!text.contains("shared configs:"), "{text}");
        assert!(!text.contains("working trees:"), "{text}");
    }

    #[test]
    fn the_same_healthy_workspace_prints_every_population_under_full() {
        // The control on the case above: without it, the silence there could be
        // a renderer that never prints these at all.
        let text = rendered_full(&with(
            vec![agent_state("arvo", AgentState::Configured, true)],
            ConfigReport {
                repos:      vec![RepoConfigState {
                    repo:       "arvo".into(),
                    local_path: "/ws/arvo".into(),
                    findings:   Vec::new(),
                    blocked:    false,
                }],
                unreadable: None,
                blocked:    false,
            },
            vec![WorktreeLine {
                repo:    "arvo".into(),
                branch:  Some("dev".into()),
                clean:   true,
                changes: 0,
                unread:  None,
            }],
        ));
        assert!(text.contains("agent surfaces:"), "{text}");
        assert!(text.contains("shared configs:"), "{text}");
        assert!(text.contains("working trees:"), "{text}");
        assert!(text.contains("clean on dev"), "{text}");
    }

    #[test]
    fn a_repo_whose_hooks_are_not_wired_shows_up_without_full() {
        // The state that went unreported entirely. It has to reach the default
        // output or the fold has bought nothing.
        let text = rendered(&with(
            vec![agent_state("kamu", AgentState::Partial, false)],
            empty_configs(),
            Vec::new(),
        ));
        assert!(text.contains("agent surfaces:"), "{text}");
        assert!(text.contains("kamu missing git-hooks"), "{text}");
    }

    #[test]
    fn a_missing_config_shows_up_without_full_and_names_the_fix() {
        let text = rendered(&with(
            Vec::new(),
            ConfigReport {
                repos:      vec![RepoConfigState {
                    repo:       "tassu".into(),
                    local_path: "/ws/tassu".into(),
                    findings:   vec!["deny.toml is missing, and is required here".into()],
                    blocked:    true,
                }],
                unreadable: None,
                blocked:    true,
            },
            Vec::new(),
        ));
        assert!(text.contains("tassu"), "{text}");
        assert!(text.contains("deny.toml is missing"), "{text}");
        assert!(text.contains("homma repo config init"), "{text}");
    }

    #[test]
    fn a_dirty_tree_shows_up_without_full_and_a_clean_one_does_not() {
        let dirty = WorktreeLine {
            repo:    "arvo".into(),
            branch:  Some("dev".into()),
            clean:   false,
            changes: 3,
            unread:  None,
        };
        let clean = WorktreeLine {
            repo:    "notko".into(),
            branch:  Some("dev".into()),
            clean:   true,
            changes: 0,
            unread:  None,
        };
        let text = rendered(&with(Vec::new(), empty_configs(), vec![dirty, clean]));
        assert!(text.contains("arvo 3 changes on dev"), "{text}");
        assert!(
            !text.contains("notko"),
            "a clean tree reached the summary: {text}"
        );
    }

    #[test]
    fn a_tree_that_could_not_be_read_is_reported_rather_than_passed_over() {
        // A member the manifest declares and nobody has cloned. Silence would
        // read as a clean tree, which is the one thing it is not.
        let text = rendered(&with(Vec::new(), empty_configs(), vec![WorktreeLine {
            repo:    "kolli".into(),
            branch:  None,
            clean:   true,
            changes: 0,
            unread:  Some("no repository at /ws/kolli".into()),
        }]));
        assert!(text.contains("kolli not read"), "{text}");
    }

    #[test]
    fn json_is_never_summarised() {
        // The property a person reading the terminal cannot notice is broken.
        // The document carries every member of every population whatever the
        // human rendering is doing, because a consumer handed one with the
        // healthy entries dropped has no way to tell.
        let r = with(
            vec![
                agent_state("arvo", AgentState::Configured, true),
                agent_state("kamu", AgentState::Partial, false),
            ],
            ConfigReport {
                repos:      vec![
                    RepoConfigState {
                        repo:       "arvo".into(),
                        local_path: "/ws/arvo".into(),
                        findings:   Vec::new(),
                        blocked:    false,
                    },
                    RepoConfigState {
                        repo:       "tassu".into(),
                        local_path: "/ws/tassu".into(),
                        findings:   vec!["deny.toml is missing, and is required here".into()],
                        blocked:    true,
                    },
                ],
                unreadable: None,
                blocked:    true,
            },
            vec![
                WorktreeLine {
                    repo:    "arvo".into(),
                    branch:  Some("dev".into()),
                    clean:   true,
                    changes: 0,
                    unread:  None,
                },
                WorktreeLine {
                    repo:    "notko".into(),
                    branch:  Some("dev".into()),
                    clean:   false,
                    changes: 2,
                    unread:  None,
                },
            ],
        );
        // The human default drops the healthy ones, which is the contrast.
        let human = rendered(&r);
        assert!(!human.contains("arvo ok"), "{human}");

        let doc = serde_json::to_value(&r).expect("the report serialises");
        assert_eq!(doc["agent"].as_array().unwrap().len(), 2, "{doc}");
        assert_eq!(
            doc["configs"]["repos"].as_array().unwrap().len(),
            2,
            "{doc}"
        );
        assert_eq!(doc["worktrees"].as_array().unwrap().len(), 2, "{doc}");
        // And the healthy ones are the members that must still be there.
        assert_eq!(doc["configs"]["repos"][0]["repo"], "arvo");
        assert_eq!(doc["worktrees"][0]["repo"], "arvo");
    }

    #[test]
    fn the_document_keeps_the_field_names_it_already_had() {
        // Additive is the whole contract with anything reading this. A rename
        // here is a break nobody would see until somebody's script stopped
        // finding a key.
        let doc = serde_json::to_value(report(Vec::new())).expect("the report serialises");
        for key in ["workspace", "forges", "repos", "injected"] {
            assert!(doc.get(key).is_some(), "`{key}` left the document: {doc}");
        }
        for key in ["agent", "configs", "worktrees"] {
            assert!(doc.get(key).is_some(), "`{key}` never arrived: {doc}");
        }
    }

    #[test]
    fn an_unreadable_configs_directory_is_said_once_rather_than_per_repo() {
        let text = rendered(&with(
            Vec::new(),
            ConfigReport {
                repos:      Vec::new(),
                unreadable: Some("no shared configs at /ws/.shared/configs".into()),
                blocked:    false,
            },
            Vec::new(),
        ));
        assert_eq!(
            text.matches("could not be read").count(),
            1,
            "the workspace-level fault was repeated: {text}"
        );
    }
}
