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
pub mod verify;

/// Dispatch the parsed CLI to the right command body.
pub fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Command::Status => {
            let cfg = load_config(&cli)?;
            status::run(&cfg, cli.output)
        }
        Command::Verify => {
            let cfg = load_config(&cli)?;
            verify::run(&cfg, cli.output)
        }
        Command::Repo { op } => match op {
            RepoOp::Status { repo } => {
                let cfg = load_config(&cli)?;
                repo::status::run(&cfg, repo, cli.output)
            }
        },
        Command::Forge { op } => match op {
            ForgeOp::Show { forge, slug } => {
                let cfg = load_config(&cli)?;
                forge::show::run(&cfg, forge, slug, cli.output)
            }
            ForgeOp::Exists { forge, slug } => {
                let cfg = load_config(&cli)?;
                forge::exists::run(&cfg, forge, slug, cli.output)
            }
        },
        Command::Migrate { repo, to } => migrate::run(repo, to, cli.output),
        Command::Archive { repo, from } => {
            archive::run(repo, from.as_deref(), cli.output)
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
