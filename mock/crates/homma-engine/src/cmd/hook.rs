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
        } => run_cmd(cli, cfg, event, args),
    }
}

/// The workspace's git hooks manifest, by `git_hooks` under `[paths]`.
fn git_hooks_file(cfg: &Config) -> Result<PathBuf> {
    Ok(crate::cmd::identity::paths_of(cfg)?
        .git_hooks
        .as_path()
        .to_path_buf())
}

fn install_cmd(cli: &Cli, cfg: &Config, repo: &str) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, Some(repo))?;
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
    match hooks::install(&root, workspace.as_ref()) {
        Ok(i) => {
            let mut lines = vec![format!(
                "registered with git-shook in {}: {}",
                root.display(),
                i.registered.join(", ")
            )];
            if workspace.is_none() {
                lines.push(format!(
                    "the workspace declares no git hooks of its own: there is no {} at its root, so the release gate alone is registered",
                    file.display()
                ));
            }
            for gone in &i.removed {
                lines.push(format!(
                    "removed {}, which an older homma left",
                    gone.display()
                ));
            }
            finish(cli, Report {
                ok: true,
                lines,
            })
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
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// What an older homma's entry in git-shook's chain calls. It installs the
/// clone git ran it in and, at the gate's event, runs the gate with git's
/// arguments and stdin; the chain runs its other entries. It fails closed where
/// the install refuses.
fn run_cmd(cli: &Cli, cfg: &Config, event: &str, args: &[String]) -> Result<Outcome> {
    // FIXME(remove_homma_hook_run_once_no_clone_carries_an_older_entrypo): the
    // verb answers only what an older homma installed, and goes once no clone has one.
    eprintln!(
        "homma: the {event} hook was written by an older homma; moving it onto git-shook, whose entries new to this clone run from the next event"
    );
    // a refusal fails closed: a hook that ran nothing must not read as passed
    let refused = |why: String| -> Result<Outcome> {
        eprintln!("homma: the {event} hook did not run: {why}");
        finish(cli, Report {
            ok:    false,
            lines: Vec::new(),
        })
    };
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !out.status.success() {
        return refused("this is not a repository git can name".to_string());
    }
    let root = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    let workspace = match hooks::workspace_hooks(&cfg.workspace.path, &git_hooks_file(cfg)?) {
        Ok(w) => w,
        Err(e) => return refused(e.to_string()),
    };
    if let Err(e) = hooks::install(&root, workspace.as_ref()) {
        return refused(e.to_string());
    }
    // the install held, so git-shook holds the path and this was an entry of its
    // chain, which runs its other entries itself: nothing is handed on, which
    // would run them twice, and only the gate the running entry was is run here
    if event != hooks::GATE_EVENT {
        return finish(cli, Report {
            ok:    true,
            lines: Vec::new(),
        });
    }
    let (program, gate) = hooks::GATE.split_first().expect("the gate names a program");
    let status = Command::new(program)
        .args(gate)
        .args(args)
        .current_dir(&root)
        .status()?;
    finish(cli, Report {
        ok:    status.success(),
        lines: Vec::new(),
    })
}
