//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma identity ...`: the identities a session can be launched as.
//!
//! An identity is a character file under `.shared/hands/<name>/`. `render`
//! generates the agent definition a session loads and maps its memories;
//! `launch` renders one and starts the session as that agent.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use homma_core::Config;
use homma_org::character::{self, identities};
use serde::Serialize;

use super::agents::GENERATED;
use super::org::DISCIPLINE;
use crate::cli::OutputFormat;
use crate::output::{HumanRender, emit};

/// Where the harness reads an agent's memory from, relative to the root.
pub const AGENT_MEMORY: &str = ".claude/agent-memory";
/// The variable naming the program that runs a session, `claude` when unset.
pub const CLAUDE_ENV: &str = "HOMMA_CLAUDE";
/// The variable a launched session finds its own identity in.
pub const IDENTITY_ENV: &str = "HOMMA_IDENTITY";

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

/// Generate the definition of each named identity, or of every one when none is named.
pub fn render(cfg: &Config, names: &[String], format: OutputFormat) -> Result<()> {
    let rendered = render_into(&cfg.workspace.path, names)?;
    emit(
        &RenderReport {
            rendered,
        },
        format,
    )?;
    Ok(())
}

fn render_into(root: &Path, names: &[String]) -> Result<Vec<String>> {
    let names = if names.is_empty() {
        identities(root).with_context(|| format!("reading {}", character::HANDS))?
    } else {
        names.to_vec()
    };
    for name in &names {
        character::render(
            root,
            name,
            DISCIPLINE,
            Path::new(GENERATED),
            Path::new(AGENT_MEMORY),
        )
        .with_context(|| format!("rendering the identity `{name}`"))?;
    }
    Ok(names)
}

/// The program, arguments and environment a session as `name` is started with.
pub fn launch_command(root: &Path, name: &str, extra: &[String]) -> Command {
    let program = std::env::var(CLAUDE_ENV).unwrap_or_else(|_| "claude".to_string());
    let mut cmd = Command::new(program);
    cmd.arg("--agent").arg(name).args(extra);
    cmd.env(IDENTITY_ENV, name).current_dir(root);
    cmd
}

/// Render `name`, then replace this process with a session running as it.
pub fn launch(cfg: &Config, name: &str, extra: &[String]) -> Result<()> {
    let root = &cfg.workspace.path;
    render_into(root, &[name.to_string()])?;
    let err = launch_command(root, name, extra).exec();
    bail!("starting the session as `{name}`: {err}")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn a_launch_names_the_agent_and_carries_the_identity_in_its_environment() {
        let cmd = launch_command(Path::new("/ws"), "coord", &["--resume".to_string()]);
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, ["--agent", "coord", "--resume"]);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(OsStr::new(IDENTITY_ENV), Some(OsStr::new("coord")))));
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/ws")));
    }
}
