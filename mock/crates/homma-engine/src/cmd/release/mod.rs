//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release`: the gate, its record and status, and the release. The
//! logic lives in `homma_core::release`; this wires it to the manifest, the
//! store, the forge client and the terminal.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use homma_api::{GateRun, Level};
use homma_core::forge::token;
use homma_core::release::gate::Real;
use homma_core::release::registry::Registry;
use homma_core::release::run::Setup;
use homma_core::release::{badges, check, hook, plan, publish, run, version};
use homma_core::{Config, Forge, RepoConfig};
use homma_store::Store;
use serde::Serialize;

use crate::cli::{Cli, HookOp, ReleaseOp};
use crate::cmd::{Outcome, config_path, load_config};
use crate::output::{HumanRender, emit};

mod gating;
mod order;
use gating::gate_cmd;
use order::{release_order, sibling_dependency};

pub mod clock;
pub mod record;

/// What every subcommand prints: lines for a person, and the same lines with
/// a verdict for a pipe.
#[derive(Debug, Serialize)]
struct Report {
    ok:    bool,
    lines: Vec<String>,
}

impl HumanRender for Report {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        write!(out, "{}", self.lines.join("\n"))
    }
}

fn finish(cli: &Cli, report: Report) -> Result<Outcome> {
    let ok = report.ok;
    emit(&report, cli.output)?;
    Ok(if ok { Outcome::Ok } else { Outcome::ReportedFailure })
}

/// The store the records go to: `.data/homma/` beside the manifest.
fn store(cli: &Cli) -> Store {
    let beside = config_path(cli);
    let dir = beside
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".data")
        .join("homma");
    Store::open(dir)
}

/// The repo named, or the one the working directory is inside, with its
/// root made absolute against the workspace.
fn resolve_repo<'a>(
    cfg: &'a Config,
    name: Option<&str>,
) -> Result<(&'a str, &'a RepoConfig, PathBuf)> {
    let root_of =
        |r: &RepoConfig| crate::cmd::util::resolve_local_path(&cfg.workspace.path, &r.local_path);
    if let Some(n) = name {
        return cfg
            .repos
            .get_key_value(n)
            .map(|(k, r)| (k.as_str(), r, root_of(r)))
            .ok_or_else(|| anyhow!("`{n}` is not a repository in this workspace"));
    }
    let here = std::env::current_dir()?;
    let here = here.canonicalize().unwrap_or(here);
    let containing = |dir: &Path| {
        cfg.repos
            .iter()
            .map(|(n, r)| (n.as_str(), r, root_of(r)))
            .filter(|(_, _, root)| {
                let p = root.canonicalize().unwrap_or_else(|_| root.clone());
                dir.starts_with(&p)
            })
            .max_by_key(|(_, _, root)| root.components().count())
    };
    if let Some(found) = containing(&here) {
        return Ok(found);
    }
    // a worktree sits beside the clones rather than under one, and a hook
    // runs with it as the working directory; the clone it hangs off is the
    // parent of the common git directory
    let common = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(&here)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
    if let Some(clone) = common.as_deref().and_then(Path::parent) {
        let clone = clone.canonicalize().unwrap_or_else(|_| clone.to_path_buf());
        if let Some(found) = containing(&clone) {
            return Ok(found);
        }
    }
    Err(anyhow!(
        "the working directory is not inside a workspace repository; name one"
    ))
}

/// The trunk and the release line for this repo: the workspace's working
/// branch onto its public one, or the public one alone where the repo has no
/// working branch, which is the shape of a crate released off `main`.
fn branches_for<'a>(cfg: &'a Config, root: &Path) -> (&'a str, &'a str) {
    let public = cfg.defaults.public_branch.as_str();
    let working = cfg.defaults.working_branch.as_str();
    // a fresh clone holds `origin/dev` and no local `dev`, and it still
    // releases by merging, so the remote ref counts as the branch being there
    let here = homma_core::release::git::sha(root, working).is_ok();
    let there = homma_core::release::git::sha(root, &format!("origin/{working}")).is_ok();
    if here || there { (working, public) } else { (public, public) }
}

/// The branches, refusing where the working branch is on origin and not
/// here: the plan reads it and the run commits on it, so a clone that has
/// not checked it out is told to, rather than quietly released off `main`.
fn branches_checked<'a>(cfg: &'a Config, root: &Path) -> Result<(&'a str, &'a str)> {
    let (trunk, release) = branches_for(cfg, root);
    if trunk != release && homma_core::release::git::sha(root, trunk).is_err() {
        return Err(anyhow!(
            "`{trunk}` is on origin and not checked out here; run `git switch {trunk}` in the \
             clone first"
        ));
    }
    Ok((trunk, release))
}

/// One line saying which branches a release of `root` carries.
fn branches_line(cfg: &Config, root: &Path) -> String {
    let (trunk, release) = branches_for(cfg, root);
    if trunk == release {
        format!("`{release}` alone, no working branch here or on origin")
    } else {
        format!("`{trunk}` onto `{release}`")
    }
}

fn forge_for(cfg: &Config, repo: &RepoConfig) -> Result<(Box<dyn Forge>, String)> {
    let name = repo
        .forge
        .as_deref()
        .ok_or_else(|| anyhow!("the repository's origin names no configured forge"))?;
    let owner = repo
        .owner
        .clone()
        .ok_or_else(|| anyhow!("the repository's origin names no owner"))?;
    let forge = crate::cmd::forge::client_from_config(crate::cmd::forge::resolve_forge(cfg, name)?);
    Ok((forge, owner))
}

fn token_source(cfg: &Config) -> impl Fn(Registry) -> std::result::Result<String, String> + '_ {
    move |r: Registry| {
        let reg = cfg
            .registry(r.key())
            .ok_or_else(|| format!("no [registries.{}] in homma.toml", r.key()))?;
        token::resolve_registry(reg).ok_or_else(|| {
            format!(
                "neither the variable nor the command in [registries.{}] gave a token",
                r.key()
            )
        })
    }
}

/// The `homma release` entry: loads the workspace and dispatches one
/// subcommand, so every path below it starts from the same manifest.
pub fn run(cli: &Cli, op: &ReleaseOp) -> Result<Outcome> {
    let cfg = load_config(cli)?;
    match op {
        ReleaseOp::Check {
            repo,
        } => check_cmd(cli, &cfg, repo.as_deref()),
        ReleaseOp::Gate {
            repo,
            sha,
            hook,
            post,
            git_args: _,
        } => {
            // under the hook the first positional is git's remote name, not
            // a repo; the repo is the one the working directory is in
            let repo = if *hook { None } else { repo.as_deref() };
            gate_cmd(cli, &cfg, repo, sha.as_deref(), *hook, post.as_deref())
        },
        ReleaseOp::Plan {
            repo,
            level,
        } => plan_cmd(cli, &cfg, repo.as_deref(), *level),
        ReleaseOp::Run {
            repo,
            level,
            dry_run,
        } => run_cmd(cli, &cfg, repo.as_deref(), *level, *dry_run),
        ReleaseOp::Badges {
            repo,
        } => badges_cmd(cli, &cfg, repo),
        ReleaseOp::Hook {
            op: HookOp::Install {
                repo,
            },
        } => hook_cmd(cli, &cfg, repo),
    }
}

fn published_for(cfg: &Config, root: &Path) -> Result<check::Published> {
    let kind = homma_core::release::kind::detect(root, &cfg.markers)?;
    let packages = check::packages(root, kind);
    Ok(check::fetch_published(&packages)?)
}

fn check_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, repo)?;
    let root = &root;
    let published = published_for(cfg, root)?;
    let findings = check::check(&check::Inputs {
        root,
        remote: "origin",
        trunk: branches_checked(cfg, root)?.0,
        release: branches_checked(cfg, root)?.1,
        level: None,
        published: &published,
        markers: &cfg.markers,
    })?;
    let lines: Vec<String> = if findings.is_empty() {
        vec!["nothing to report".into()]
    } else {
        findings
            .iter()
            .map(|f| {
                format!(
                    "{:<5} {:<22} {}",
                    format!("{:?}", f.severity).to_ascii_lowercase(),
                    f.id,
                    f.message
                )
            })
            .collect()
    };
    finish(cli, Report {
        ok: !check::blocked(&findings),
        lines,
    })
}

fn plan_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>, level: Level) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, repo)?;
    let p = plan::plan(
        &root,
        &cfg.markers,
        branches_checked(cfg, &root)?.0,
        level,
        &clock::today(),
    )?;
    finish(cli, Report {
        ok:    true,
        lines: vec![branches_line(cfg, &root), p.to_string()],
    })
}

fn run_cmd(
    cli: &Cli,
    cfg: &Config,
    repo: Option<&str>,
    level: Level,
    dry_run: bool,
) -> Result<Outcome> {
    let store = store(cli);
    let token = token_source(cfg);
    let names: Vec<String> = match repo {
        Some(n) => vec![n.to_string()],
        None => release_order(cfg),
    };
    let mut lines = Vec::new();
    // a workspace-wide run first decides which repos it will release: a repo
    // the sweep passes over is named, with why, since a silent skip is how
    // three stack repos went unreleased without a word
    let names: Vec<String> = if repo.is_none() {
        let mut active = Vec::new();
        for name in &names {
            let (_, _, root) = resolve_repo(cfg, Some(name))?;
            let (trunk, _) = branches_checked(cfg, &root)?;
            match plan::plan(&root, &cfg.markers, trunk, level, &clock::today()) {
                Ok(p) if p.commits.is_empty() => {
                    lines.push(format!("{name}: nothing unreleased, passed over"));
                },
                Ok(_) => active.push(name.clone()),
                // a manifest off the level refuses a single run, so it
                // refuses the sweep too rather than being passed over
                Err(e @ plan::PlanError::OffLevel(_)) => {
                    return Err(anyhow!("{name}: {e}"));
                },
                Err(e) => lines.push(format!("{name}: passed over, {e}")),
            }
        }
        // the sweep goes in name order, and a dependent released ahead of
        // what it depends on has its tag and forge release pushed before the
        // publish fails, so an edge between two repos it would release
        // refuses the run; an edge to a repo it passes over is no edge, and
        // a repo whose own member crate carries its name is not its own edge
        for name in &active {
            let (_, _, root) = resolve_repo(cfg, Some(name))?;
            let others: Vec<String> = active.iter().filter(|n| *n != name).cloned().collect();
            if let Some(dep) = sibling_dependency(&root, &others) {
                return Err(anyhow!(
                    "`{name}` depends on `{dep}`, and a workspace-wide release goes in name order; \
                     release `{dep}` first, then `{name}`, each by name"
                ));
            }
        }
        active
    } else {
        names
    };
    let mut ok = true;
    for name in &names {
        let (name, r, root) = resolve_repo(cfg, Some(name))?;
        let root = &root;
        let (trunk, release_line) = branches_checked(cfg, root)?;
        let (forge, owner) = forge_for(cfg, r)?;
        let published = published_for(cfg, root)?;
        let tip = homma_core::release::git::sha(root, trunk)?;
        let newest = record::newest_for(&store, name, &tip)?;
        let date = clock::today();
        let setup = Setup {
            runner: &Real,
            forge: forge.as_ref(),
            owner: &owner,
            name,
            remote: "origin",
            trunk,
            release: release_line,
            date: &date,
            token: &token,
            served: &publish::registry_serves,
            published: &published,
            markers: &cfg.markers,
        };
        match run::release(&setup, root, level, newest.as_ref(), dry_run) {
            Ok(Ok(done)) => {
                lines.push(format!(
                    "{name}: released {} as `{}` at {}",
                    done.plan.next,
                    done.plan.tag,
                    &done.tag_sha[.. 7]
                ));
            },
            Ok(Err(p)) => lines.push(format!("{name}: {}\n{p}", branches_line(cfg, root))),
            Err(e) => {
                ok = false;
                lines.push(format!("{name}: {e}"));
                break;
            },
        }
    }
    if lines.is_empty() {
        lines.push("nothing unreleased".into());
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

fn badges_cmd(cli: &Cli, cfg: &Config, repo: &str) -> Result<Outcome> {
    let (name, _, root) = resolve_repo(cfg, Some(repo))?;
    let root = &root;
    let store = store(cli);
    // the newest record, whichever commit it measured: runs are recorded
    // for the checked-out head, which is the trunk, and the release line's
    // tip is a merge commit no gate ever ran on
    let run: GateRun = record::newest(&store, name)?.ok_or_else(|| {
        anyhow!("no gate run recorded for `{name}`; push it through the hook or run `homma release gate`")
    })?;
    let kind = homma_core::release::kind::detect(root, &cfg.markers)?;
    let v = version::read(root, kind)?;
    let files = badges::files(&run, &v);
    let sha = badges::write(root, &files)?;
    homma_core::release::git::push(
        root,
        "origin",
        &format!("refs/heads/{}", badges::BRANCH),
        true,
    )?;
    finish(cli, Report {
        ok:    true,
        lines: vec![format!(
            "wrote {} file(s) to `{}` at {}, from the run on {}",
            files.len(),
            badges::BRANCH,
            &sha[.. 7],
            &run.sha[.. 7]
        )],
    })
}

fn hook_cmd(cli: &Cli, cfg: &Config, repo: &str) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, Some(repo))?;
    match hook::install(&root) {
        Ok(i) => {
            finish(cli, Report {
                ok:    true,
                lines: vec![format!("wrote {}", i.path.display())],
            })
        },
        // a refusal is reported, a line and a non-zero exit, so a sweep
        // across the workspace goes on to the next repo
        Err(
            e @ (hook::HookError::HooksPathOutside(_)
            | hook::HookError::HooksPathTracked(_)
            | hook::HookError::HookExists(_)),
        ) => {
            finish(cli, Report {
                ok:    false,
                lines: vec![e.to_string()],
            })
        },
        // named, so a refusal added later has no arm here and does not
        // compile, rather than aborting a sweep as an error
        Err(e @ (hook::HookError::Git(_) | hook::HookError::Io(_))) => Err(e.into()),
    }
}
