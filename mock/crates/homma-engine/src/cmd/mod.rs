//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Command bodies. One module per top-level subcommand.
//!
//! Every command body takes the parsed [`crate::cli::Cli`] plus any
//! command-specific args, performs its work, and returns an
//! `anyhow::Result<()>`. Successful output goes to stdout via
//! [`crate::output::emit`]; errors propagate to `main` which writes them
//! to stderr and exits non-zero.

use anyhow::{Context, Result};
use homma_core::Config;

use crate::cli::{AgentOp, Cli, Command, ConfigOp, DocsOp, ForgeOp, OrgOp, RepoOp};

pub mod agent;
pub mod aggregate;
pub mod archive;
pub mod config;
pub mod docs;
#[cfg(test)]
pub mod fake_git;
pub mod forge;
pub mod gates;
pub mod migrate;
pub mod org;
pub mod registry;
pub mod release;
pub mod repo;
pub mod stand;
pub mod status;
pub(crate) mod util;
pub mod verify;

/// Outcome of dispatching a command body.
///
/// Most commands either succeed or fail outright; some (notably `verify`)
/// run to completion, emit a structured report, and need the process to
/// exit non-zero. [`Outcome::ReportedFailure`] is the latter case: the
/// command already wrote its diagnostic to stdout; `main` translates this
/// into [`std::process::ExitCode::FAILURE`] without writing anything to
/// stderr (the report is the message). This keeps every exit through the
/// `main` return path so destructors and tracing flushes run normally.
pub enum Outcome {
    /// Command completed successfully. Exit 0.
    Ok,
    /// Command completed, emitted its own diagnostic, and signals a
    /// non-zero exit. Used by `verify` when checks fail.
    ReportedFailure,
}

/// Dispatch the parsed CLI to the right command body.
pub fn run(cli: Cli) -> Result<Outcome> {
    match &cli.command {
        Command::Status {
            full,
        } => {
            let cfg = load_config(&cli)?;
            status::run(&cfg, *full, cli.output)
        },
        Command::Org {
            op,
        } => {
            let path = config_path(&cli);
            let ws = org::load(&path)?;
            match op {
                OrgOp::List => {
                    for line in org::list(&ws) {
                        println!(
                            "{:<12} {:<8} {:<28} {}",
                            line.handle,
                            format!("{:?}", line.role).to_lowercase(),
                            org::describe(&line.staffing),
                            line.domain,
                        );
                    }
                },
                OrgOp::Add {
                    handle,
                    role,
                    nickname,
                    full_name,
                    domain,
                    staffed,
                    git_name,
                    git_email,
                    workspace,
                } => {
                    let mut ws = ws;
                    let mut id = homma_api::Identity::new((*role).into(), handle.clone());
                    id.staffed = *staffed;
                    id.nickname = nickname.clone();
                    id.full_name = full_name.clone();
                    id.domain = domain.clone();
                    id.git_name = git_name.clone();
                    id.git_email = git_email.clone();
                    id.workspace = workspace.clone();
                    let staffing = id.staffing();
                    org::add(&mut ws, id.clone())?;

                    // Appended, so every comment and every hand-chosen ordering
                    // in the registry survives being added to. Written through a
                    // temporary and renamed, so a short write cannot leave a
                    // registry nothing can parse.
                    // The registry is written at a path the operator named,
                    // so it is checked like every other operator-named path.
                    //
                    // **Against the whole list, which it was not.** This passed
                    // `Denied::from_env()` alone, which is one of the three
                    // kinds of denied place, so a registry could be rewritten
                    // inside a participant's workspace while the readme said it
                    // was checked against the same list as everything else.
                    //
                    // `for_standing_up` still cannot serve: it needs a standee
                    // and nobody is being stood up. What this needs is every
                    // participant's workspace with no exclusion, since the
                    // registry belongs to the workspace it configures rather
                    // than to any of them, and that is what
                    // `for_the_workspace` is.
                    let base = homma_api::AbsPath::new(
                        std::path::absolute(&path)
                            .unwrap_or_else(|_| path.clone())
                            .parent()
                            .unwrap_or(std::path::Path::new("/"))
                            .to_path_buf(),
                    )
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let denied = homma_api::Denied::for_the_workspace(&ws, &base)?;
                    registry::append_entry(&path, &id, &denied)?;

                    println!("{} {}", id.handle, org::describe(&staffing));
                },
                OrgOp::Up {
                    handle,
                    root,
                } => {
                    // The configuration file's own directory, never the current
                    // one. The current directory says where somebody happened to
                    // be standing; treating that as the workspace cloned an
                    // arbitrary repository and wrote a participant's directories
                    // into whatever tree the operator was in.
                    let root = root.clone().unwrap_or_else(|| {
                        path.parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                    });
                    // The one place a relative path becomes absolute, and the
                    // only place that resolution is a judgement rather than a
                    // type: everything downstream takes `AbsPath`.
                    let root = match homma_api::AbsPath::new(&root) {
                        Ok(p) => p,
                        Err(_) => homma_api::AbsPath::resolve(&homma_api::AbsPath::cwd()?, &root),
                    }
                    .canonical()
                    .with_context(|| format!("resolving the workspace root {}", root.display()))?;
                    let out = stand::stand_up(&ws, &root, handle, &homma_core::repo::GixGit)?;
                    println!("{} {}", out.handle, out.home);
                    println!(
                        "  workspace  {} ({})",
                        out.workspace.display(),
                        if out.cloned { "cloned" } else { "already there" }
                    );
                    println!("  definition {}", out.definition);
                    println!("  twin       {}", out.twin_definition);
                },
            }
            Ok(Outcome::Ok)
        },
        Command::Verify {
            forge,
        } => {
            let cfg = load_config(&cli)?;
            verify::run(&cfg, *forge, cli.output)
        },
        Command::Repo {
            op,
        } => {
            match op {
                RepoOp::Status {
                    repo,
                } => {
                    let cfg = load_config(&cli)?;
                    repo::status::run(&cfg, repo, cli.output)?;
                    Ok(Outcome::Ok)
                },
                RepoOp::Config {
                    op,
                } => {
                    let cfg = load_config(&cli)?;
                    match op {
                        ConfigOp::Check {
                            repo,
                        } => config::check(&cfg, repo.as_deref(), cli.output),
                        ConfigOp::Init {
                            repo,
                        } => config::init(&cfg, repo.as_deref(), cli.output),
                    }
                },
            }
        },
        Command::Forge {
            op,
        } => {
            match op {
                ForgeOp::Show {
                    forge,
                    slug,
                } => {
                    let cfg = load_config(&cli)?;
                    forge::show::run(&cfg, forge, slug, cli.output)?;
                    Ok(Outcome::Ok)
                },
                ForgeOp::Exists {
                    forge,
                    slug,
                } => {
                    let cfg = load_config(&cli)?;
                    forge::exists::run(&cfg, forge, slug, cli.output)?;
                    Ok(Outcome::Ok)
                },
            }
        },
        Command::Migrate {
            repo,
            to,
            to_owner,
            to_org,
            source,
            dry_run,
        } => {
            let cfg = load_config(&cli)?;
            let opts = migrate::Opts {
                to_owner: to_owner.as_deref(),
                to_org:   *to_org,
                source:   source.as_deref(),
                dry_run:  *dry_run,
            };
            migrate::run(&cfg, repo, to, opts, cli.output)
        },
        Command::Archive {
            repo,
            from,
            owner,
        } => {
            let cfg = load_config(&cli)?;
            archive::run(&cfg, repo, from.as_deref(), owner.as_deref(), cli.output)?;
            Ok(Outcome::Ok)
        },
        Command::Agent {
            op,
        } => {
            match op {
                AgentOp::Status {
                    repo,
                } => {
                    let cfg = load_config(&cli)?;
                    agent::status::run(&cfg, repo.as_deref(), cli.output)?;
                    Ok(Outcome::Ok)
                },
                AgentOp::Regen {
                    repo,
                    continue_on_error,
                    skip_cargo_mock,
                    skip_configs,
                    skip_aggregate,
                } => {
                    let cfg = load_config(&cli)?;
                    agent::regen::run_with(
                        &cfg,
                        repo.as_deref(),
                        agent::regen::Opts {
                            continue_on_error: *continue_on_error,
                            skip_cargo_mock:   *skip_cargo_mock,
                            skip_configs:      *skip_configs,
                            skip_aggregate:    *skip_aggregate,
                        },
                        cli.output,
                    )
                },
            }
        },
        Command::Release {
            op,
        } => release::run(&cli, op),
        Command::Docs {
            op,
        } => {
            match op {
                DocsOp::Status {
                    repo,
                } => {
                    let cfg = load_config(&cli)?;
                    docs::status::run(&cfg, repo.as_deref(), cli.output)?;
                    Ok(Outcome::Ok)
                },
            }
        },
    }
}

/// Where the configuration is, from the two flags that can say so.
///
/// `--config` names the file and wins. `--dir` names the directory it sits in,
/// which is what the launcher passes, resolved absolutely, so the command
/// operates on the same workspace whichever directory it was typed in. With
/// neither, the bare name is left relative and resolves against the current
/// directory, which is the shape a hand-run command in a workspace root wants.
///
/// One function rather than a computation repeated per command: the `org` arm
/// carried its own copy and would have kept reading `./homma.toml` while every
/// other command honoured `--dir`.
pub(crate) fn config_path(cli: &Cli) -> std::path::PathBuf {
    if let Some(path) = &cli.config {
        return path.clone();
    }
    match &cli.dir {
        Some(dir) => dir.join("homma.toml"),
        None => std::path::PathBuf::from("homma.toml"),
    }
}

/// Resolve the config path and parse it.
pub(crate) fn load_config(cli: &Cli) -> Result<Config> {
    let path = config_path(cli);
    Config::from_path(&path).with_context(|| format!("loading config from {}", path.display()))
}

#[cfg(test)]
mod config_path_tests {
    use clap::Parser;

    use super::*;

    /// Parse a `homma-engine` command line the way `main` does.
    fn parsed(args: &[&str]) -> Cli {
        let mut argv = vec!["homma-engine"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv).expect("these arguments should parse")
    }

    #[test]
    fn the_dir_flag_is_accepted_at_all() {
        // The launcher passes it unconditionally, so an engine that rejects it
        // cannot be run through the launcher at all. That is how it was found:
        // `error: unexpected argument '--dir' found`, on every subcommand.
        let cli = parsed(&["--dir", "/tmp/ws", "status"]);
        assert_eq!(cli.dir.as_deref(), Some(std::path::Path::new("/tmp/ws")));
    }

    #[test]
    fn the_config_sits_in_the_directory_the_dir_flag_names() {
        let cli = parsed(&["--dir", "/tmp/ws", "status"]);
        assert_eq!(
            config_path(&cli),
            std::path::PathBuf::from("/tmp/ws/homma.toml")
        );
    }

    #[test]
    fn a_named_config_wins_over_the_directory() {
        let cli = parsed(&["--dir", "/tmp/ws", "-c", "/elsewhere/other.toml", "status"]);
        assert_eq!(
            config_path(&cli),
            std::path::PathBuf::from("/elsewhere/other.toml")
        );
    }

    #[test]
    fn with_neither_flag_the_bare_name_resolves_against_the_current_directory() {
        let cli = parsed(&["status"]);
        let path = config_path(&cli);
        assert_eq!(path, std::path::PathBuf::from("homma.toml"));
        assert!(
            path.is_relative(),
            "a hand-run command in a workspace root reads that root's config"
        );
    }

    #[test]
    fn the_flag_parses_through_a_nested_subcommand_too() {
        // `org list` is two levels down, and a flag that is global on the root
        // but not inherited would be rejected there. This is about parsing;
        // that the `org` body actually honours it is
        // `runs_from_anywhere.rs`, which runs the binary.
        let cli = parsed(&["--dir", "/tmp/ws", "org", "list"]);
        assert_eq!(
            config_path(&cli),
            std::path::PathBuf::from("/tmp/ws/homma.toml")
        );
    }

    #[test]
    fn the_flag_is_taken_after_the_subcommand_too() {
        // Which is how a user types it, and how anything forwarding arguments
        // through a subcommand would end up ordering them.
        let cli = parsed(&["status", "--dir", "/tmp/ws"]);
        assert_eq!(
            config_path(&cli),
            std::path::PathBuf::from("/tmp/ws/homma.toml")
        );
    }
}
