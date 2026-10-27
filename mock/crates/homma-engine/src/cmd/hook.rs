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
use crate::cmd::release::{Report, finish};
use crate::cmd::util::resolve_repo;

pub fn run(cli: &Cli, cfg: &Config, op: &HookOp) -> Result<Outcome> {
    match op {
        HookOp::Install {
            repo,
        } => install_cmd(cli, cfg, repo),
        HookOp::Doctor {
            repo,
        } => doctor_cmd(cli, cfg, repo.as_deref()),
        HookOp::Run {
            event,
            args,
        } => run_cmd(cli, cfg, event, args),
    }
}

/// The report a hook verb gives instead of acting while a layer `layers`
/// names could not be read. The load goes on past a broken overlay so
/// `homma local` can repair one; what the verbs must not do is install or run
/// the table with a layer's hooks missing from it, since a check that did not
/// run looks the same as one that passed.
fn refused_for_layers(cfg: &Config) -> Option<Report> {
    if cfg.layer_errors.is_empty() {
        return None;
    }
    let mut lines = cfg.layer_errors.clone();
    lines.push("the hooks table is incomplete without it, so nothing is installed or run".into());
    Some(Report {
        ok: false,
        lines,
    })
}

fn install_cmd(cli: &Cli, cfg: &Config, repo: &str) -> Result<Outcome> {
    if let Some(report) = refused_for_layers(cfg) {
        return finish(cli, report);
    }
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
        Err(e @ (hooks::HookError::HookExists(_) | hooks::HookError::Shook(_))) => {
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

fn doctor_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>) -> Result<Outcome> {
    let mut ok = true;
    let mut lines = Vec::new();
    let survey = homma_core::layers::survey(&cfg.layers_dir, &cfg.layers)?;
    lines.push(format!(
        "hooks are read from homma.toml, then {}",
        if cfg.layers.names().is_empty() {
            "no layers".to_string()
        } else {
            cfg.layers.names().join(", ")
        }
    ));
    for name in &survey.read {
        // a file that cannot be read is listed once, as the error it is
        if cfg.layer_errors.iter().any(|e| e.contains(name.as_str())) {
            continue;
        }
        lines.push(format!("read      {name}"));
    }
    for name in &survey.absent {
        lines.push(format!("absent    {name} (named, and not there)"));
    }
    for name in &survey.strays {
        ok = false;
        lines.push(format!(
            "stray     {name} is not named in `layers`, so it is never read; name it there or remove it"
        ));
    }
    for err in &cfg.layer_errors {
        ok = false;
        lines.push(format!("error     {err}"));
    }
    if let Some(repo) = repo {
        let (_, _, root) = resolve_repo(cfg, Some(repo))?;
        for event in cfg.hooks.events() {
            let reach = hooks::reach(&root, event)?;
            ok &= reach.reached();
            lines.push(format!("{event}: {reach}"));
        }
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

fn run_cmd(cli: &Cli, cfg: &Config, event: &str, args: &[String]) -> Result<Outcome> {
    if let Some(report) = refused_for_layers(cfg) {
        return finish(cli, report);
    }
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
