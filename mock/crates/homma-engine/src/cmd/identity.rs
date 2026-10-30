//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma identity ...`: the identities a session can be launched as.
//!
//! An identity is a character file in the hands directory. `render` generates
//! the agent definition a session loads and maps its memories; `launch` renders
//! one and starts the session as that agent.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use homma_api::{AbsPath, Paths};
use homma_core::Config;
use homma_org::Layout;
use homma_org::agents::Personas;
use homma_org::character::{self, claimed_names};
use serde::Serialize;

use super::agent::denied_for_aggregating;
use super::agents::authored_dir;
use super::org::DISCIPLINE;
use super::tools::dispatch::exec;
use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// The variable naming the program that runs a session, `claude` when unset.
pub const CLAUDE_ENV: &str = "HOMMA_CLAUDE";

/// What the render pass wrote.
#[derive(Debug, Serialize)]
pub struct RenderReport {
    pub rendered: Vec<String>,
}

impl HumanRender for RenderReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        if self.rendered.is_empty() {
            return write!(out, "no identities are authored here");
        }
        write!(
            out,
            "{} identities: {}",
            self.rendered.len(),
            self.rendered.join(", ")
        )
    }
}

/// The `[paths]` the configuration names, or the defaults when it names none.
fn paths_of(cfg: &Config) -> Result<Paths> {
    match &cfg.paths {
        Some(v) => v.clone().try_into().context("reading `[paths]`"),
        None => Ok(Paths::default()),
    }
}

/// Run `f` over the layout of the workspace `cfg` resolved, with the same
/// places denied that `agent regen` denies.
pub fn with_layout<T>(cfg: &Config, f: impl FnOnce(&Layout<'_>) -> Result<T>) -> Result<T> {
    let workspace = &cfg.workspace.path;
    let root = AbsPath::new(std::path::absolute(workspace).unwrap_or_else(|_| workspace.clone()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let paths = paths_of(cfg)?;
    let layout = Layout::new(&root, &paths, denied_for_aggregating(cfg, &root)?)
        .with_context(|| format!("resolving the workspace root at {root}"))?;
    f(&layout)
}

/// Refuse a persona and an identity of one name: each would write
/// `.claude/agents/<name>.md`, and whichever ran last would win unannounced.
///
/// Asked of the claimed names, not of the checked identities: a persona must not
/// collide with an identity whose character is broken either, and a broken one
/// is `identity render`'s to report.
pub fn refuse_collisions(layout: &Layout<'_>, personas: &Personas) -> Result<()> {
    let ids = claimed_names(layout);
    for p in &personas.personas {
        if ids.contains(&p.name) {
            bail!(
                "`{}` is both an authored persona and an identity, and both generate \
                 `.claude/agents/{}.md`",
                p.name,
                p.name
            );
        }
    }
    Ok(())
}

/// The authored personas, none when there is no corpus here.
fn personas(cfg: &Config) -> Result<Option<Personas>> {
    let dir = authored_dir(cfg);
    if !dir.is_dir() {
        return Ok(None);
    }
    Ok(Some(super::agents::load(&dir)?))
}

/// Generate the definition of each named identity, or of every one when none is named.
pub fn render(cfg: &Config, names: &[String], format: OutputFormat) -> Result<()> {
    let rendered = render_into(cfg, names)?;
    emit(
        &RenderReport {
            rendered,
        },
        format,
    )?;
    Ok(())
}

fn render_into(cfg: &Config, names: &[String]) -> Result<Vec<String>> {
    let personas = personas(cfg)?;
    with_layout(cfg, |layout| {
        if let Some(p) = &personas {
            refuse_collisions(layout, p)?;
        }
        let done =
            character::render(layout, names, DISCIPLINE).context("rendering the identities")?;
        Ok(done.into_iter().map(|r| r.name).collect())
    })
}

/// The program, arguments and working directory a session as `name` is started
/// with. The harness takes the identity from `--agent`, so nothing else names it.
pub fn launch_command(root: &Path, name: &str, extra: &[String]) -> Command {
    let program = std::env::var(CLAUDE_ENV).unwrap_or_else(|_| "claude".to_string());
    let mut cmd = Command::new(program);
    cmd.arg("--agent").arg(name).args(extra).current_dir(root);
    cmd
}

/// Render `name`, then replace this process with a session running as it.
pub fn launch(cfg: &Config, name: &str, extra: &[String]) -> Result<()> {
    // The harness reads definitions from `.claude/agents` only, so a definition
    // written to a configured place would be rendered and never found.
    let agents = paths_of(cfg)?.agents;
    if agents != Paths::default().agents {
        bail!(
            "`[paths] agents` is `{agents}` and the harness reads definitions from \
             `.claude/agents` only, so a session launched as `{name}` would not find its own"
        );
    }
    render_into(cfg, &[name.to_string()])?;
    match exec(launch_command(&cfg.workspace.path, name, extra)) {
        Err(e) => Err(e.context(format!("starting the session as `{name}`"))),
        Ok(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn a_launch_names_the_agent_and_runs_in_the_workspace() {
        let cmd = launch_command(Path::new("/ws"), "coord", &["--resume".to_string()]);
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, ["--agent", "coord", "--resume"]);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/ws")));
    }
}
