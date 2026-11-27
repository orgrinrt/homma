//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma hook`: the git hooks registered with git-shook, homma's release gate
//! and the workspace's git hooks manifest, and what an older homma's
//! entrypoints call until a clone is installed again. The logic is
//! `homma_core::hooks`; this wires it to the manifest and the terminal.

use std::path::PathBuf;
use std::process::Command;

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
        } => run_cmd(cli, event, args),
    }
}

/// The workspace's git hooks manifest, by `git_hooks` under `[paths]`.
fn git_hooks_file(cfg: &Config) -> Result<PathBuf> {
    Ok(crate::cmd::identity::paths_of(cfg)?
        .git_hooks
        .as_path()
        .to_path_buf())
}

fn install_cmd(cli: &Cli, cfg: &Config, repo: &Option<String>) -> Result<Outcome> {
    let file = git_hooks_file(cfg)?;
    let workspace = match hooks::workspace_hooks(&cfg.workspace.path, &file) {
        Ok(w) => w,
        Err(e) => {
            return finish(cli, Report {
                ok:    false,
                lines: vec![e.to_string()],
            });
        },
    };
    let Some(repo) = repo else {
        return sweep(cli, cfg, workspace.as_ref());
    };
    let (_, _, root) = resolve_repo(cfg, Some(repo))?;
    let mut lines = Vec::new();
    let ok = install_one(&root, workspace.as_ref(), &mut lines)?;
    if ok && workspace.is_none() {
        lines.push(format!(
            "the workspace declares no git hooks of its own: there is no {} at its root, so the release gate alone is registered",
            file.display()
        ));
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// Every member whose registration is not current installed again, one line
/// for each; nothing written or said for one that is.
fn sweep(cli: &Cli, cfg: &Config, workspace: Option<&hooks::WorkspaceHooks>) -> Result<Outcome> {
    let mut ok = true;
    let mut lines = Vec::new();
    for name in cfg.repos.keys() {
        let (_, _, root) = resolve_repo(cfg, Some(name))?;
        let stale = match hooks::registration(&root, workspace, None) {
            Ok(stale) => stale,
            Err(hooks::HookError::NotShook(_)) => {
                ok &= install_one(&root, workspace, &mut lines)?;
                continue;
            },
            Err(e) => return Err(e.into()),
        };
        if stale.is_empty() {
            continue;
        }
        ok &= install_one(&root, workspace, &mut lines)?;
    }
    // every member current: a session start hears nothing, not even a blank line
    if ok && lines.is_empty() && matches!(cli.output, crate::cli::OutputFormat::Human) {
        return Ok(Outcome::Ok);
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// Install at `root`, saying what was done or why not; whether it went.
fn install_one(
    root: &std::path::Path,
    workspace: Option<&hooks::WorkspaceHooks>,
    lines: &mut Vec<String>,
) -> Result<bool> {
    match hooks::install(root, workspace) {
        Ok(i) => {
            lines.push(format!(
                "registered with git-shook in {}: {}",
                root.display(),
                i.registered.join(", ")
            ));
            for gone in &i.removed {
                lines.push(format!(
                    "removed {}, which an older homma left",
                    gone.display()
                ));
            }
            Ok(true)
        },
        // a refusal is reported, a line and a non-zero exit, so a sweep
        // across the workspace goes on to the next repo
        Err(
            e @ (hooks::HookError::NotShook(_)
            | hooks::HookError::Shook(_)
            | hooks::HookError::Manifest {
                ..
            }
            | hooks::HookError::Name {
                ..
            }),
        ) => {
            lines.push(e.to_string());
            Ok(false)
        },
        // named, so a refusal added later has no arm here and does not
        // compile, rather than aborting a sweep as an error
        Err(e @ (hooks::HookError::Git(_) | hooks::HookError::Io(_))) => Err(e.into()),
    }
}

fn doctor_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>) -> Result<Outcome> {
    let mut ok = true;
    let mut lines = Vec::new();
    let file = git_hooks_file(cfg)?;
    match hooks::workspace_hooks(&cfg.workspace.path, &file) {
        Ok(Some(w)) => {
            lines.push(format!(
                "git hooks are declared in {}, registered as {}",
                w.path.display(),
                w.name
            ));
        },
        Ok(None) => {
            lines.push(format!(
                "git hooks: no {} at the workspace root, so only the release gate is registered",
                file.display()
            ));
        },
        Err(e) => {
            ok = false;
            lines.push(format!("error     {e}"));
        },
    }
    for retired in &cfg.retired {
        ok = false;
        lines.push(format!("retired   {}", retired.said(&file)));
    }
    let survey = homma_core::layers::survey(&cfg.layers_dir, &cfg.layers)?;
    lines.push(format!(
        "agent hooks and nudges are read from homma.toml, then {}",
        if cfg.layers.names().is_empty() {
            "no layers".to_string()
        } else {
            cfg.layers.names().join(", ")
        }
    ));
    for name in &survey.read {
        // a file that cannot be read is listed once, as the error it is
        if homma_core::layers::read(&cfg.layers_dir, name).is_err() {
            continue;
        }
        lines.push(format!("read      {name}"));
    }
    for name in &survey.absent {
        lines.push(format!("absent    {name} (named, and not there)"));
    }
    if survey.own_unlisted {
        lines.push(format!(
            "note      {} is not named in `layers`, so its agent tables are not read; its own tables still are, so it stays",
            homma_api::layers::DEFAULT_LAYER
        ));
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
        if hooks::holds_path(&root)? {
            lines.push(format!(
                "{repo}: git-shook holds the hooks path; `git shook list` shows what it registered"
            ));
        } else {
            ok = false;
            lines.push(format!(
                "{repo}: git-shook does not hold the hooks path, so nothing homma registers runs; run `git shook install` there, then `homma hook install {repo}`"
            ));
        }
        for left in hooks::legacy::left_behind(&root)? {
            ok = false;
            lines.push(format!(
                "{repo}: {} was left by an older homma; `homma hook install {repo}` removes it",
                left.display()
            ));
        }
        // what git-shook says of the two manifests, where it holds the path to say it
        let workspace = hooks::workspace_hooks(&cfg.workspace.path, &file)
            .ok()
            .flatten();
        if let Ok(stale) = hooks::registration(&root, workspace.as_ref(), None) {
            for line in stale.iter().filter(|l| l.starts_with("git shook doctor:")) {
                ok = false;
                lines.push(format!(
                    "{repo}: {line}; `homma hook install {repo}` puts it back"
                ));
            }
        }
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// What an older homma's entrypoint calls. At the gate's event it runs the
/// release gate, with git's arguments and stdin, as the old table's first row
/// did; at every event it says to install again; at any other it runs nothing.
fn run_cmd(cli: &Cli, event: &str, args: &[String]) -> Result<Outcome> {
    // FIXME(remove_homma_hook_run_once_no_clone_carries_an_older_entrypo): the
    // verb answers only what an older homma installed, and goes once no clone has one.
    eprintln!(
        "homma: the {event} hook was written by an older homma; `homma hook install <repo>` moves it onto git-shook"
    );
    if event != hooks::GATE_EVENT {
        return finish(cli, Report {
            ok:    true,
            lines: Vec::new(),
        });
    }
    let (program, gate) = hooks::GATE.split_first().expect("the gate names a program");
    let status = Command::new(program).args(gate).args(args).status()?;
    finish(cli, Report {
        ok:    status.success(),
        lines: Vec::new(),
    })
}
