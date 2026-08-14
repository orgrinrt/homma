//! Command bodies. One module per top-level subcommand.
//!
//! Every command body takes the parsed [`crate::cli::Cli`] plus any
//! command-specific args, performs its work, and returns an
//! `anyhow::Result<()>`. Successful output goes to stdout via
//! [`crate::output::emit`]; errors propagate to `main` which writes them
//! to stderr and exits non-zero.

use anyhow::{Context, Result};
use homma_core::Config;

use crate::cli::{AgentOp, Cli, Command, DocsOp, ForgeOp, OrgOp, RepoOp};

pub mod agent;
pub mod aggregate;
pub mod archive;
pub mod docs;
pub mod forge;
pub mod gates;
pub mod migrate;
pub mod org;
pub mod repo;
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
        Command::Status => {
            let cfg = load_config(&cli)?;
            status::run(&cfg, cli.output)?;
            Ok(Outcome::Ok)
        }
        Command::Org { op } => {
            let path = cli
                .config
                .clone()
                .unwrap_or_else(|| std::path::PathBuf::from("homma.toml"));
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
                }
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
                    org::append_entry(&path, &id)?;

                    println!("{} {}", id.handle, org::describe(&staffing));
                }
                OrgOp::Up { handle, root } => {
                    let root = root
                        .clone()
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let out = org::stand_up(&ws, &root, handle, &homma_core::repo::GixGit)?;
                    println!("{} {}", out.handle, out.home.display());
                    println!(
                        "  workspace  {} ({})",
                        out.workspace.display(),
                        if out.cloned { "cloned" } else { "already there" }
                    );
                    println!("  definition {}", out.definition.display());
                    println!("  twin       {}", out.twin_definition.display());
                }
            }
            Ok(Outcome::Ok)
        }
        Command::Verify => {
            let cfg = load_config(&cli)?;
            verify::run(&cfg, cli.output)
        }
        Command::Repo { op } => match op {
            RepoOp::Status { repo } => {
                let cfg = load_config(&cli)?;
                repo::status::run(&cfg, repo, cli.output)?;
                Ok(Outcome::Ok)
            }
        },
        Command::Forge { op } => match op {
            ForgeOp::Show { forge, slug } => {
                let cfg = load_config(&cli)?;
                forge::show::run(&cfg, forge, slug, cli.output)?;
                Ok(Outcome::Ok)
            }
            ForgeOp::Exists { forge, slug } => {
                let cfg = load_config(&cli)?;
                forge::exists::run(&cfg, forge, slug, cli.output)?;
                Ok(Outcome::Ok)
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
                to_org: *to_org,
                source: source.as_deref(),
                dry_run: *dry_run,
            };
            migrate::run(&cfg, repo, to, opts, cli.output)
        }
        Command::Archive { repo, from, owner } => {
            let cfg = load_config(&cli)?;
            archive::run(&cfg, repo, from.as_deref(), owner.as_deref(), cli.output)?;
            Ok(Outcome::Ok)
        }
        Command::Agent { op } => match op {
            AgentOp::Status { repo } => {
                let cfg = load_config(&cli)?;
                agent::status::run(&cfg, repo.as_deref(), cli.output)?;
                Ok(Outcome::Ok)
            }
            AgentOp::Regen {
                repo,
                continue_on_error,
                skip_cargo_mock,
                skip_aggregate,
            } => {
                let cfg = load_config(&cli)?;
                agent::regen::run_with(
                    &cfg,
                    repo.as_deref(),
                    agent::regen::Opts {
                        continue_on_error: *continue_on_error,
                        skip_cargo_mock: *skip_cargo_mock,
                        skip_aggregate: *skip_aggregate,
                    },
                    cli.output,
                )
            }
        },
        Command::Docs { op } => match op {
            DocsOp::Status { repo } => {
                let cfg = load_config(&cli)?;
                docs::status::run(&cfg, repo.as_deref(), cli.output)?;
                Ok(Outcome::Ok)
            }
        },
    }
}

/// Resolve the config path (CLI flag wins; otherwise `./homma.toml`) and parse.
pub(crate) fn load_config(cli: &Cli) -> Result<Config> {
    let path = cli
        .config
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("homma.toml"));
    Config::from_path(&path).with_context(|| format!("loading config from {}", path.display()))
}
