//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! From `homma <tool> <words>` to the process that runs.
//!
//! The first word after the tool's name is a verb when the descriptor names a command that way. When it
//! does not, the command named like the tool takes every word, which is how a tool whose first argument
//! is not a verb (`msg kaski board`) is a subcommand. Anything else is refused with the verbs listed, and
//! a tool whose descriptor is broken is refused by name with the reason, never by silence.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::descriptor::{Entry, Tool};

/// What runs: the executable and the words it is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The tool it belongs to.
    pub tool:    String,
    /// The executable, absolute.
    pub program: PathBuf,
    /// The words after the verb.
    pub args:    Vec<OsString>,
}

/// How the tool was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// As a subcommand of homma, which only a promoted tool answers.
    AsSubcommand,
    /// By `homma tools run`, which any tool in the workspace answers.
    ByName,
}

fn verbs_of(tool: &Tool) -> String {
    let names: Vec<&str> = tool.verbs.iter().map(|v| v.name.as_str()).collect();
    if names.is_empty() { "none".to_string() } else { names.join(", ") }
}

/// The command to run for `tool` given `words`.
///
/// # Errors
///
/// When no tool of that name is there, its descriptor is broken, it is not promoted and was asked for as
/// a subcommand, its backend is not `local`, no verb or default command fits the words, or the file the
/// command runs is not an executable file.
pub fn plan(entries: &[Entry], name: &str, words: &[OsString], asked: Asked) -> Result<Plan> {
    let Some(entry) = entries.iter().find(|e| e.name() == name) else {
        bail!(
            "there is no command or tool named `{name}`; `homma --help` lists the commands and `homma tools` the tools"
        );
    };
    let tool = match entry {
        Entry::Broken(b) => {
            bail!(
                "the tool `{}` is broken: {} {}",
                b.name,
                b.file.display(),
                b.why
            )
        },
        Entry::Tool(t) => t,
    };
    if asked == Asked::AsSubcommand && !tool.promote {
        bail!(
            "the tool `{name}` is not promoted, so it is no subcommand of homma; `homma tools run {name} ...` runs it"
        );
    }
    if tool.backend != "local" {
        bail!(
            "the tool `{name}` has the backend `{}`, which homma does not run; only `local` tools, the ones in the workspace, are run here",
            tool.backend
        );
    }
    let first = words.first().and_then(|w| w.to_str());
    let (verb, rest) = match first.and_then(|f| tool.verbs.iter().find(|v| v.name == f)) {
        Some(v) => (v, &words[1 ..]),
        None => {
            match tool.verbs.iter().find(|v| v.name == tool.name) {
                Some(default) => (default, words),
                None => {
                    match first {
                        Some(f) => {
                            bail!(
                                "the tool `{name}` has no verb `{f}`; its verbs are {}",
                                verbs_of(tool)
                            )
                        },
                        None => {
                            bail!(
                                "the tool `{name}` wants a verb; its verbs are {}",
                                verbs_of(tool)
                            )
                        },
                    }
                },
            }
        },
    };
    let program = tool.dir.join(&verb.run);
    check_runnable(&program, name, &verb.name)?;
    Ok(Plan {
        tool: tool.name.clone(),
        program,
        args: rest.to_vec(),
    })
}

fn check_runnable(program: &Path, tool: &str, verb: &str) -> Result<()> {
    let Ok(meta) = std::fs::metadata(program) else {
        bail!(
            "the command `{verb}` of the tool `{tool}` runs {}, which is not there",
            program.display()
        );
    };
    if !meta.is_file() {
        bail!(
            "the command `{verb}` of the tool `{tool}` runs {}, which is not a file",
            program.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            bail!(
                "the command `{verb}` of the tool `{tool}` runs {}, which is not executable",
                program.display()
            );
        }
    }
    Ok(())
}

/// The process for a plan: run from the workspace root, told where it is.
pub fn command(plan: &Plan, root: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(&plan.program);
    c.args(&plan.args)
        .current_dir(root)
        .env("HOMMA_WORKSPACE", root);
    c
}

/// Replace homma with the plan's process, so its input, output and exit status are its own.
///
/// # Errors
///
/// When the process could not be started. A started one does not return.
pub fn exec(plan: &Plan, root: &Path) -> Result<std::convert::Infallible> {
    let mut c = command(plan, root);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = c.exec();
        bail!("could not start {}: {e}", plan.program.display())
    }
    #[cfg(not(unix))]
    {
        let status = c
            .status()
            .map_err(|e| anyhow::anyhow!("could not start {}: {e}", plan.program.display()))?;
        std::process::exit(status.code().unwrap_or(1))
    }
}
