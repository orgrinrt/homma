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
//!
//! Where the tool is, and the process that runs it, are renki's: [`locate`] finds it against the backends
//! renki ships and [`command`] builds the process, with its working directory, `HOMMA_WORKSPACE` and
//! `HOMMA_TOOL_ROOT`, and refuses a command that leaves the tool.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use renki::extension::{BUILTIN, Descriptor, Registry, command, locate};

use super::Asked;
use super::descriptor::Entry;

/// The backends renki ships, `local` and `git`. A tool naming another is refused with these listed.
pub static REGISTRY: Registry = Registry::new(BUILTIN);

/// The host's short name, which the variables the process is told are named for.
const SHORT: &str = "homma";

fn verbs_of(tool: &Descriptor) -> String {
    let names: Vec<&str> = tool.commands.iter().map(|c| c.name.as_str()).collect();
    if names.is_empty() { "none".to_string() } else { names.join(", ") }
}

/// The process to run for the tool `name` given `words`, ready to be started.
///
/// `workspace` is where it runs and where a `local` tool's `source` is read from. `cache` is asked for
/// the shared cache only when the tool's backend keeps one, so a machine with no home directory still
/// runs its local tools.
///
/// # Errors
///
/// When no tool of that name is there, its descriptor is broken, it is not promoted and was asked for as
/// a subcommand, no verb or default command fits the words, renki cannot find the tool (a backend it has
/// no row for, a source that is not there, a fetch that failed) or refuses the command (a file that is not
/// there or resolves outside the tool), or the file is not executable.
pub fn plan(
    entries: &[Entry],
    name: &str,
    words: &[OsString],
    asked: Asked,
    workspace: &Path,
    cache: &dyn Fn() -> Result<PathBuf, String>,
) -> Result<Command> {
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
        Entry::Tool(d) => d,
    };
    if asked == Asked::AsSubcommand && !tool.promote {
        bail!(
            "the tool `{name}` is not promoted, so it is no subcommand of homma; `homma tools run {name} ...` runs it"
        );
    }
    let first = words.first().and_then(|w| w.to_str());
    let (verb, rest) = match first.and_then(|f| tool.command(f)) {
        Some(v) => (v, &words[1 ..]),
        None => {
            match tool.command(&tool.name) {
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
    // The cache is asked for by the backends that keep one and by no other, which is what `locate` does
    // with it too; a backend renki has no row for is `locate`'s refusal to make.
    let cache = match REGISTRY.get(&tool.backend) {
        Some(b) if b.caches() => cache().map_err(|e| anyhow!(e))?,
        _ => PathBuf::new(),
    };
    let located = locate(tool, &REGISTRY, workspace, &cache).map_err(|e| anyhow!(e))?;
    // The caller's words go on as they are: `command` takes text, and a word that is not text is still
    // the caller's.
    let mut c =
        command(tool, &located, &verb.name, SHORT, workspace, &[]).map_err(|e| anyhow!(e))?;
    check_executable(&c, name, &verb.name)?;
    c.args(rest);
    Ok(c)
}

/// renki's `command` knows the file is one and that it stays inside the tool, and not that it can be run.
fn check_executable(c: &Command, tool: &str, verb: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let program = Path::new(c.get_program());
    let mode = std::fs::metadata(program)?.permissions().mode();
    if mode & 0o111 == 0 {
        bail!(
            "the command `{verb}` of the tool `{tool}` runs {}, which is not executable",
            program.display()
        );
    }
    Ok(())
}

/// Replace homma with the process, so its input, output and exit status are its own.
///
/// # Errors
///
/// When the process could not be started. A started one does not return.
pub fn exec(mut c: Command) -> Result<std::convert::Infallible> {
    use std::os::unix::process::CommandExt;
    let e = c.exec();
    bail!(
        "could not start {}: {e}",
        Path::new(c.get_program()).display()
    )
}
