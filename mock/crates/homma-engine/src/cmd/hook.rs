//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma hook`: the entrypoints, one per event the table names, and what an
//! entrypoint calls. The logic is `homma_core::hooks`; this wires it to the
//! manifest and the terminal.

use std::io::{IsTerminal, Read};

use anyhow::Result;
use homma_core::{Config, hooks};

use crate::cli::{Cli, HookOp};
use crate::cmd::Outcome;
use crate::cmd::release::{Report, finish, resolve_repo};

pub fn run(cli: &Cli, cfg: &Config, op: &HookOp) -> Result<Outcome> {
    match op {
        HookOp::Install {
            repo,
        } => install_cmd(cli, cfg, repo),
        HookOp::Run {
            event,
            args,
        } => run_cmd(cli, cfg, event, args),
    }
}

fn install_cmd(cli: &Cli, cfg: &Config, repo: &str) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, Some(repo))?;
    match hooks::install(&root, &cfg.hooks) {
        Ok(i) => {
            // one line per entrypoint, with how git reaches it; one that
            // nothing reaches makes the exit non-zero like a refusal, so a
            // sweep across the workspace goes on and the operator sees which
            // repositories and events git will not run
            let lines: Vec<String> = i
                .written
                .iter()
                .map(|w| format!("wrote {}: {}", w.path.display(), w.reach))
                .collect();
            finish(cli, Report {
                ok: i.reached(),
                lines,
            })
        },
        // a refusal is reported, a line and a non-zero exit, so a sweep
        // across the workspace goes on to the next repo
        Err(e @ hooks::HookError::HookExists(_)) => {
            finish(cli, Report {
                ok:    false,
                lines: vec![e.to_string()],
            })
        },
        // named, so a refusal added later has no arm here and does not
        // compile, rather than aborting a sweep as an error
        Err(e @ (hooks::HookError::Git(_) | hooks::HookError::Io(_))) => Err(e.into()),
    }
}

fn run_cmd(cli: &Cli, cfg: &Config, event: &str, args: &[String]) -> Result<Outcome> {
    // the entrypoint runs with the repository as its working directory, and
    // a worktree's clone is found through its common git directory
    let (_, _, root) = resolve_repo(cfg, None)?;
    // git hands some events their input on stdin; read it once, for those
    // events and no other, and replay it to every entry. A hook for any other
    // event may be handed an open pipe by whatever wraps git, and a read there
    // would wait on it; a terminal is never read either, so a person typing
    // the verb by hand is not left waiting on input git would have closed.
    let mut stdin = String::new();
    if homma_api::hooks::STDIN_EVENTS.contains(&event) && !std::io::stdin().is_terminal() {
        std::io::stdin().read_to_string(&mut stdin)?;
    }
    let ran = hooks::run(&root, &cfg.hooks, event, args, &stdin)?;
    finish(cli, Report {
        ok:    ran.ok,
        lines: ran.lines,
    })
}
