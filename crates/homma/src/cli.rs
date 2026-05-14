//! `clap` argument shape for the `homma` CLI.
//!
//! Defined in one place so command bodies under [`crate::cmd`] receive
//! structured `Args` values instead of re-parsing `std::env::args`. The
//! global options (`--config`, `--output`, `-v / -vv / -vvv`) sit on the
//! root [`Cli`] and apply to every subcommand.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// `homma`: workspace management CLI for multi-repo Rust workspaces.
#[derive(Debug, Parser)]
#[command(name = "homma", version, about, long_about = None)]
pub struct Cli {
    /// Path to `homma.toml` (default: `./homma.toml`).
    ///
    /// When the file does not exist or fails to parse, commands that read
    /// config (everything except `--help` / `--version`) exit non-zero with
    /// an explanatory diagnostic on stderr.
    #[arg(long, short = 'c', global = true)]
    pub config: Option<PathBuf>,

    /// Output format. Human is the default; JSON emits one document per
    /// command to stdout for machine consumption.
    #[arg(long, value_enum, global = true, default_value_t = OutputFormat::Human)]
    pub output: OutputFormat,

    /// Verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. Without any
    /// `-v` flag the default is the `warn` level.
    ///
    /// The `RUST_LOG` env var overrides this when set; use it for finer
    /// per-module filters (e.g. `RUST_LOG=homma=trace,gix=info`).
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count)]
    pub verbosity: u8,

    #[command(subcommand)]
    pub command: Command,
}

/// Output format for command results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Plain text, designed for terminal readers.
    Human,
    /// One JSON document per command, suitable for piping into `jq`.
    Json,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show workspace state: parsed `homma.toml`, repos and their configured
    /// forges, default-branch resolution.
    Status,

    /// Sanity-check `homma.toml`: parses, repo `local_path`s exist (when
    /// the workspace root is present), forge `token_env` vars resolve.
    /// Exits non-zero with a per-finding diagnostic list when checks fail.
    Verify,

    /// Per-repo operations over the local working tree.
    Repo {
        #[command(subcommand)]
        op: RepoOp,
    },

    /// Forge operations: read repo metadata, check existence.
    Forge {
        #[command(subcommand)]
        op: ForgeOp,
    },

    /// Migrate a repo from one configured forge to another. Stub: substantive
    /// behaviour lands in #452 alongside the two-phase migrate command.
    Migrate {
        /// Repo name from `homma.toml` (the key under `[repos.<name>]`).
        repo: String,
        /// Target forge profile name from `homma.toml` (the key under
        /// `[forges.<name>]`).
        #[arg(long)]
        to: String,
    },

    /// Archive a source-side repo after a successful migration. Stub:
    /// substantive behaviour lands in #452.
    Archive {
        /// Repo name from `homma.toml`.
        repo: String,
        /// Source forge profile name. Defaults to the repo's currently
        /// configured forge.
        #[arg(long)]
        from: Option<String>,
    },
}

/// `repo` subcommands.
#[derive(Debug, Subcommand)]
pub enum RepoOp {
    /// Show working-tree status (branch, dirty/clean, ahead/behind) for
    /// the named repo.
    Status {
        /// Repo name from `homma.toml`.
        repo: String,
    },
}

/// `forge` subcommands.
#[derive(Debug, Subcommand)]
pub enum ForgeOp {
    /// Fetch repo metadata from the configured forge.
    Show {
        /// Forge profile name from `homma.toml`.
        forge: String,
        /// `<owner>/<name>`.
        slug: String,
    },
    /// Check whether a repo exists on the configured forge. Exits 0 with
    /// `exists: true` when present, 0 with `exists: false` when absent.
    /// Network or auth failures exit non-zero.
    Exists {
        /// Forge profile name from `homma.toml`.
        forge: String,
        /// `<owner>/<name>`.
        slug: String,
    },
}

impl ForgeOp {
    /// Split a `<owner>/<name>` string into its parts.
    pub fn parse_slug(slug: &str) -> Result<(&str, &str), SlugError> {
        let mut parts = slug.splitn(2, '/');
        let owner = parts.next().filter(|s| !s.is_empty()).ok_or(SlugError::Empty)?;
        let name = parts.next().filter(|s| !s.is_empty()).ok_or(SlugError::Missing)?;
        if name.contains('/') {
            return Err(SlugError::TooManyParts);
        }
        Ok((owner, name))
    }
}

/// Parse error for a `<owner>/<name>` slug argument.
#[derive(Debug)]
pub enum SlugError {
    /// The slug was empty or started with `/`.
    Empty,
    /// The slug had no `/` separator.
    Missing,
    /// The slug had more than one `/`.
    TooManyParts,
}

impl std::fmt::Display for SlugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "slug is empty; expected `<owner>/<name>`"),
            Self::Missing => {
                write!(f, "slug missing `/`; expected `<owner>/<name>`")
            }
            Self::TooManyParts => {
                write!(f, "slug has too many `/`; expected `<owner>/<name>`")
            }
        }
    }
}

impl std::error::Error for SlugError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slug_happy_path() {
        let (o, n) = ForgeOp::parse_slug("orgrinrt/homma").unwrap();
        assert_eq!(o, "orgrinrt");
        assert_eq!(n, "homma");
    }

    #[test]
    fn parse_slug_empty_fails() {
        assert!(matches!(ForgeOp::parse_slug(""), Err(SlugError::Empty)));
    }

    #[test]
    fn parse_slug_no_separator_fails() {
        assert!(matches!(ForgeOp::parse_slug("homma"), Err(SlugError::Missing)));
    }

    #[test]
    fn parse_slug_three_parts_fails() {
        assert!(matches!(
            ForgeOp::parse_slug("a/b/c"),
            Err(SlugError::TooManyParts)
        ));
    }

    #[test]
    fn parse_slug_trailing_slash_fails() {
        assert!(matches!(ForgeOp::parse_slug("a/"), Err(SlugError::Missing)));
    }

    #[test]
    fn parse_slug_leading_slash_fails() {
        assert!(matches!(ForgeOp::parse_slug("/a"), Err(SlugError::Empty)));
    }

    #[test]
    fn cli_parses_basic_invocation() {
        let cli = Cli::try_parse_from(["homma", "status"]).unwrap();
        assert!(matches!(cli.command, Command::Status));
        assert_eq!(cli.output, OutputFormat::Human);
        assert_eq!(cli.verbosity, 0);
    }

    #[test]
    fn cli_parses_global_options() {
        let cli = Cli::try_parse_from([
            "homma", "-vv", "--output", "json", "-c", "alt.toml", "verify",
        ])
        .unwrap();
        assert_eq!(cli.verbosity, 2);
        assert_eq!(cli.output, OutputFormat::Json);
        assert_eq!(cli.config.as_deref().map(|p| p.to_str().unwrap()), Some("alt.toml"));
        assert!(matches!(cli.command, Command::Verify));
    }

    #[test]
    fn cli_parses_forge_show() {
        let cli = Cli::try_parse_from([
            "homma", "forge", "show", "github", "orgrinrt/homma",
        ])
        .unwrap();
        match cli.command {
            Command::Forge { op: ForgeOp::Show { forge, slug } } => {
                assert_eq!(forge, "github");
                assert_eq!(slug, "orgrinrt/homma");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_migrate() {
        let cli = Cli::try_parse_from(["homma", "migrate", "notko", "--to", "codeberg"]).unwrap();
        match cli.command {
            Command::Migrate { repo, to } => {
                assert_eq!(repo, "notko");
                assert_eq!(to, "codeberg");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_rejects_unknown_subcommand() {
        let err = Cli::try_parse_from(["homma", "nope"]).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("unrecognized") || s.contains("invalid"), "got: {s}");
    }
}
