//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release`: the gate, its record and status, and the release. The
//! logic lives in `homma_core::release`; this wires it to the manifest, the
//! store, the forge client and the terminal.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use homma_api::{GateRun, Level, Verdict};
use homma_core::forge::token;
use homma_core::release::gate::{self, Real};
use homma_core::release::registry::Registry;
use homma_core::release::run::Setup;
use homma_core::release::{badges, check, hook, plan, publish, run, status, version};
use homma_core::{Config, Forge, RepoConfig};
use homma_store::Store;
use serde::Serialize;

use crate::cli::{Cli, HookOp, ReleaseOp};
use crate::cmd::{Outcome, config_path, load_config};
use crate::output::{HumanRender, emit};

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
fn resolve_repo<'a>(cfg: &'a Config, name: Option<&str>) -> Result<(&'a str, &'a RepoConfig, PathBuf)> {
    let root_of = |r: &RepoConfig| crate::cmd::util::resolve_local_path(&cfg.workspace.path, &r.local_path);
    if let Some(n) = name {
        return cfg
            .repos
            .get_key_value(n)
            .map(|(k, r)| (k.as_str(), r, root_of(r)))
            .ok_or_else(|| anyhow!("`{n}` is not a repository in this workspace"));
    }
    let here = std::env::current_dir()?;
    let here = here.canonicalize().unwrap_or(here);
    cfg.repos
        .iter()
        .map(|(n, r)| (n.as_str(), r, root_of(r)))
        .filter(|(_, _, root)| {
            let p = root.canonicalize().unwrap_or_else(|_| root.clone());
            here.starts_with(&p)
        })
        .max_by_key(|(_, _, root)| root.components().count())
        .ok_or_else(|| anyhow!("the working directory is not inside a workspace repository; name one"))
}

/// The trunk and the release line for this repo: the workspace's working
/// branch onto its public one, or the public one alone where the repo has no
/// working branch, which is the shape of a crate released off `main`.
fn branches_for<'a>(cfg: &'a Config, root: &Path) -> (&'a str, &'a str) {
    let public = cfg.defaults.public_branch.as_str();
    let working = cfg.defaults.working_branch.as_str();
    if homma_core::release::git::sha(root, working).is_ok() {
        (working, public)
    } else {
        (public, public)
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
        } => gate_cmd(cli, &cfg, repo.as_deref(), sha.as_deref(), *hook, post.as_deref()),
        ReleaseOp::Plan {
            repo,
            level,
        } => plan_cmd(cli, &cfg, repo, *level),
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

fn published_for(root: &Path) -> Result<check::Published> {
    let kind = homma_core::release::kind::detect(root)?;
    let packages = check::packages(root, kind);
    Ok(check::fetch_published(&packages)?)
}

fn check_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, repo)?;
    let root = &root;
    let published = published_for(root)?;
    let findings = check::check(&check::Inputs {
        root,
        remote: "origin",
        trunk: branches_for(cfg, root).0,
        release: branches_for(cfg, root).1,
        level: None,
        published: &published,
    })?;
    let lines: Vec<String> = if findings.is_empty() {
        vec!["nothing to report".into()]
    } else {
        findings
            .iter()
            .map(|f| format!("{:<5} {:<22} {}", format!("{:?}", f.severity).to_ascii_lowercase(), f.id, f.message))
            .collect()
    };
    finish(cli, Report {
        ok: !check::blocked(&findings),
        lines,
    })
}

fn gate_cmd(
    cli: &Cli,
    cfg: &Config,
    repo: Option<&str>,
    sha: Option<&str>,
    hook: bool,
    post: Option<&str>,
) -> Result<Outcome> {
    let (name, r, root) = resolve_repo(cfg, repo)?;
    let root = &root;
    let store = store(cli);
    let (forge, owner) = forge_for(cfg, r)?;
    if let Some(sha) = post {
        let run = record::newest_for(&store, name, sha)?
            .ok_or_else(|| anyhow!("no gate run recorded on {sha}"))?;
        status::post(forge.as_ref(), &owner, name, &run)
            .with_context(|| format!("posting the status on {sha}"))?;
        return finish(cli, Report {
            ok:    true,
            lines: vec![format!("posted {} on {sha}: {}", status::CONTEXT, status::description(&run))],
        });
    }
    let head = homma_core::release::git::head(root)?;
    if hook && !pushing_head(&head)? {
        return finish(cli, Report {
            ok:    true,
            lines: vec![format!("{} is not among the refs being pushed; nothing to gate", &head[.. 7])],
        });
    }
    if let Some(want) = sha {
        if !head.starts_with(want) {
            return Err(anyhow!("the checkout is at {head}, not {want}; the gate measures the tree it is given"));
        }
    }
    let run = gate::run_gate(&Real, root, name, &clock::now())?;
    record::append(&store, &run).context("recording the run")?;
    let mut lines = vec![run.summary()];
    match status::post(forge.as_ref(), &owner, name, &run) {
        Ok(()) => lines.push(format!("posted {} on {}", status::CONTEXT, &run.sha[.. 7])),
        Err(e) => {
            lines.push(format!(
                "the status was not posted ({e}); the record is kept and `homma release gate --post {}` posts it",
                &run.sha[.. 7]
            ))
        },
    }
    finish(cli, Report {
        ok: run.verdict == Verdict::Green,
        lines,
    })
}

/// Whether the tip is among the refs a pre-push hook is handed on stdin,
/// one `<local ref> <local sha> <remote ref> <remote sha>` per line.
fn pushing_head(head: &str) -> Result<bool> {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
    Ok(text
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .any(|s| s == head))
}

fn plan_cmd(cli: &Cli, cfg: &Config, repo: &str, level: Level) -> Result<Outcome> {
    let (_, _, root) = resolve_repo(cfg, Some(repo))?;
    let p = plan::plan(&root, branches_for(cfg, &root).0, level, &clock::today())?;
    finish(cli, Report {
        ok:    true,
        lines: vec![p.to_string()],
    })
}

fn run_cmd(cli: &Cli, cfg: &Config, repo: Option<&str>, level: Level, dry_run: bool) -> Result<Outcome> {
    let store = store(cli);
    let token = token_source(cfg);
    let names: Vec<String> = match repo {
        Some(n) => vec![n.to_string()],
        // FIXME: every repo goes in name order; the dependency order across
        // repos the deep dive asks for needs the cross-repo graph, which
        // nothing reads yet.
        None => cfg.repos.keys().cloned().collect(),
    };
    let mut lines = Vec::new();
    let mut ok = true;
    for name in &names {
        let (name, r, root) = resolve_repo(cfg, Some(name))?;
        let root = &root;
        let (trunk, release_line) = branches_for(cfg, root);
        if repo.is_none() {
            let Ok(p) = plan::plan(root, trunk, level, &clock::today()) else {
                continue;
            };
            if p.commits.is_empty() {
                continue;
            }
        }
        let (forge, owner) = forge_for(cfg, r)?;
        let published = published_for(root)?;
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
            Ok(Err(p)) => lines.push(format!("{name}:\n{p}")),
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
    let tip = homma_core::release::git::sha(root, &cfg.defaults.public_branch)?;
    let run: GateRun = record::newest_for(&store, name, &tip)?
        .ok_or_else(|| anyhow!("no gate run recorded on the tip of `{}`", cfg.defaults.public_branch))?;
    let kind = homma_core::release::kind::detect(root)?;
    let v = version::read(root, kind)?;
    let files = badges::files(&run, &v);
    let sha = badges::write(root, &files)?;
    homma_core::release::git::push(root, "origin", &format!("refs/heads/{}", badges::BRANCH), true)?;
    finish(cli, Report {
        ok:    true,
        lines: vec![format!("wrote {} file(s) to `{}` at {}", files.len(), badges::BRANCH, &sha[.. 7])],
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
        Err(e @ hook::HookError::HooksPathOutside(_)) => {
            finish(cli, Report {
                ok:    false,
                lines: vec![e.to_string()],
            })
        },
        Err(e) => Err(e.into()),
    }
}
