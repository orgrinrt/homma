//! Command bodies. One module per top-level subcommand.
//!
//! Every command body takes the parsed [`crate::cli::Cli`] plus any
//! command-specific args, performs its work, and returns an
//! `anyhow::Result<()>`. Successful output goes to stdout via
//! [`crate::output::emit`]; errors propagate to `main` which writes them
//! to stderr and exits non-zero.

use anyhow::{Context, Result};
use homma_core::Config;

use crate::cli::{Cli, Command, ForgeOp, RepoOp};

pub mod archive;
pub mod forge;
pub mod migrate;
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
        Command::Migrate { repo, to, to_owner, to_org, source, dry_run } => {
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
    }
}

/// Resolve the config path (CLI flag wins; otherwise `./homma.toml`) and parse.
pub(crate) fn load_config(cli: &Cli) -> Result<Config> {
    let path = cli
        .config
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("homma.toml"));
    Config::from_path(&path)
        .with_context(|| format!("loading config from {}", path.display()))
}
