//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

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
    /// Path to `homma.toml`. Wins over `--dir` when both are given.
    ///
    /// When the file does not exist or fails to parse, commands that read
    /// config (everything except `--help` / `--version`) exit non-zero with
    /// an explanatory diagnostic on stderr.
    #[arg(long, short = 'c', global = true)]
    pub config: Option<PathBuf>,

    /// The workspace root, which is the directory `homma.toml` sits in.
    ///
    /// The launcher always passes this, resolved absolutely, so which
    /// directory the command was typed in never changes what it operates on.
    /// Typed by hand it does the same thing for the same reason.
    #[arg(long, global = true)]
    pub dir: Option<PathBuf>,

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

    /// The registry: who exists, and standing them up.
    ///
    /// Reads the identities from the workspace configuration. Standing one up
    /// creates its directories, links its memory where the agent harness looks
    /// for it, and generates the definitions it and its twin run under.
    Org {
        #[command(subcommand)]
        op: OrgOp,
    },

    /// Sanity-check `homma.toml`: parses, declares the forges its repos name,
    /// and resolves each forge's `token_env`. Exits non-zero with a per-finding
    /// diagnostic list when checks fail.
    Verify {
        /// Also ask each forge whether the repo exists under the owner and name
        /// the manifest gives it.
        ///
        /// Off by default because it is one network round-trip per repo and
        /// the rest of the command is offline. It is the only check that
        /// catches a wrong `owner`, which is otherwise invisible until a forge
        /// operation 404s.
        #[arg(long)]
        forge: bool,
    },

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

    /// Workspace-level mockspace agent-template orchestration.
    ///
    /// Each member repo renders its own `.claude/` and `.github/` from
    /// `mock/agent/` templates via `cargo mock`. `homma agent` walks the
    /// workspace and either discovers per-repo state (`status`) or
    /// drives the regen in each (`regen`).
    Agent {
        #[command(subcommand)]
        op: AgentOp,
    },

    /// Workspace-level documentation discovery.
    ///
    /// Reports which documentation surfaces (`README.md`, `docs/`, design
    /// templates, `CHANGELOG.md`) each member repo currently has. Aggregating
    /// and rendering them is not built.
    Docs {
        #[command(subcommand)]
        op: DocsOp,
    },

    /// The workspace's own rule corpus: what governs a subject, and the cards.
    ///
    /// Rules are injected into every session the workspace runs, sub-agents
    /// included, so their size is paid before any work starts. Each is authored
    /// under `.shared/rules/` as one template: the meta as frontmatter, then the
    /// card, then a marker, then the elaboration. The card a session loads is
    /// that prefix, generated.
    Rules {
        #[command(subcommand)]
        op: RulesOp,
    },

    /// The workspace's skills: what is authored, and the tree a session finds.
    ///
    /// A skill's body is fetched on demand and costs a session nothing until it
    /// is. Its description is different: the listing carries every one of them
    /// on every session, so that is the field with a budget on it. Authored
    /// under `.shared/skills/`, generated into `.claude/skills/`.
    Skills {
        #[command(subcommand)]
        op: SkillsOp,
    },

    /// Migrate a repo from one configured forge to another.
    ///
    /// Reads source metadata, creates the destination repo (with description,
    /// visibility, and default branch replicated), mirror-clones the source,
    /// and pushes the mirror to the destination. Does not archive or delete
    /// the source; that is a deliberate second step via `homma archive`.
    ///
    /// The source forge is the one detected from the clone's origin remote unless `--source`
    /// overrides it. The source owner and repo name come from
    /// the detected owner and the repo's directory name respectively unless overridden.
    Migrate {
        /// The repository's directory name under the workspace root.
        repo:     String,
        /// Target forge profile name from `homma.toml`.
        #[arg(long)]
        to:       String,
        /// Destination owner. Defaults to the owner detected from the clone's origin.
        #[arg(long)]
        to_owner: Option<String>,
        /// Destination owner is an organisation (not a user account). Drives
        /// the create-repo endpoint dispatch on Forgejo / Gitea; GitHub
        /// ignores this (the token's user is implied for `POST /user/repos`).
        #[arg(long)]
        to_org:   bool,
        /// Source forge override. Defaults to the forge detected from the clone's origin.
        #[arg(long)]
        source:   Option<String>,
        /// Plan only; do not create the destination or push. Emits the
        /// migration plan and exits 0.
        #[arg(long)]
        dry_run:  bool,
    },

    /// Archive a repo on its source forge after a successful migration.
    ///
    /// Issues the forge's archive API (read-only flag). Does not delete; the
    /// source repo stays visible as a frozen artefact. Run only after the
    /// destination is verified.
    Archive {
        /// Repo name from `homma.toml`.
        repo:  String,
        /// Forge profile name. Defaults to the forge detected from the clone's origin.
        #[arg(long)]
        from:  Option<String>,
        /// Owner override. Defaults to the owner detected from the clone's origin.
        #[arg(long)]
        owner: Option<String>,
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

/// `agent` subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentOp {
    /// Discover per-repo mockspace agent-template state.
    ///
    /// For each member repo (or one when `--repo` is set), reports whether
    /// `mock/`, `.claude/`, `.github/`, and the `cargo mock` alias are
    /// present.
    Status {
        /// Single repo from `homma.toml`. Default: all.
        #[arg(long)]
        repo: Option<String>,
    },
    /// Regenerate per-repo agent surfaces and aggregate them at the
    /// workspace level.
    ///
    /// Two stages:
    /// 1. Run `cargo mock` in each member repo with a `mock/`
    ///    directory, regenerating `.claude/rules/*.md` and
    ///    `.claude/hooks/*.sh` from `mock/agent/` templates.
    /// 2. Aggregate per-repo rules and hooks into the workspace
    ///    `.claude/`, with each repo's rules and hooks scoped to only
    ///    activate when work touches that repo's tree.
    ///
    /// Repos without a `mock/` directory are skipped, not failed. Use
    /// `--skip-cargo-mock` to re-aggregate without re-rendering
    /// per-repo, or `--skip-aggregate` to just rerun `cargo mock`.
    Regen {
        /// Single repo from `homma.toml`. Default: all repos with a `mock/`.
        #[arg(long)]
        repo:              Option<String>,
        /// Keep going after a per-repo regen failure.
        #[arg(long)]
        continue_on_error: bool,
        /// Skip the per-repo `cargo mock` step; only run workspace
        /// aggregation against the already-rendered per-repo `.claude/`.
        #[arg(long)]
        skip_cargo_mock:   bool,
        /// Skip comparing each repo against the shared tool configs.
        #[arg(long)]
        skip_configs:      bool,
        /// Skip the workspace aggregation step; only run per-repo
        /// `cargo mock`.
        #[arg(long)]
        skip_aggregate:    bool,
    },
}

/// `docs` subcommands.
#[derive(Debug, Subcommand)]
pub enum DocsOp {
    /// Discover per-repo doc surfaces.
    ///
    /// Reports which of `README.md`, `docs/`, `mock/DESIGN.md.tmpl`,
    /// `mock/PRINCIPLES.md.tmpl`, `mock/WORKFLOW.md.tmpl`, and
    /// `CHANGELOG.md` each member repo currently ships.
    Status {
        /// Single repo from `homma.toml`. Default: all.
        #[arg(long)]
        repo: Option<String>,
    },
}

/// `rules` subcommands.
#[derive(Debug, Subcommand)]
pub enum RulesOp {
    /// Which rules govern a subject.
    ///
    /// For a caller who does not know the filename and would not think to look
    /// for it: ask the subject, get the governing set. Matches the topics a
    /// rule declares, not its body, which is what `find` is for.
    ///
    /// `homma rules about "writing, readme, public"`
    About {
        /// Subjects, separated by commas or spaces.
        query: String,
    },

    /// Generate the cards a session loads, from the authored templates.
    ///
    /// Writes `.claude/rules/<name>.md` per rule. Those are generated output
    /// and editing one by hand loses the edit on the next run.
    Render {},
}

/// `skills` subcommands.
#[derive(Debug, Subcommand)]
pub enum SkillsOp {
    /// Every skill, and the sentence saying when to reach for it.
    List {},

    /// Generate the tree a session finds, from the authored templates.
    ///
    /// A `.md.tmpl` is rendered and loses that suffix; everything else is
    /// copied with its mode, since a skill's scripts are not prose. Each
    /// skill's generated directory is rewritten whole, so editing one by hand
    /// loses the edit on the next run.
    Render {},
}

/// `forge` subcommands.
#[derive(Debug, Subcommand)]
pub enum ForgeOp {
    /// Fetch repo metadata from the configured forge.
    Show {
        /// Forge profile name from `homma.toml`.
        forge: String,
        /// `<owner>/<name>`.
        slug:  String,
    },
    /// Check whether a repo exists on the configured forge. Exits 0 with
    /// `exists: true` when present, 0 with `exists: false` when absent.
    /// Network or auth failures exit non-zero.
    Exists {
        /// Forge profile name from `homma.toml`.
        forge: String,
        /// `<owner>/<name>`.
        slug:  String,
    },
}

impl ForgeOp {
    /// Split a `<owner>/<name>` string into its parts.
    pub fn parse_slug(slug: &str) -> Result<(&str, &str), SlugError> {
        let mut parts = slug.splitn(2, '/');
        let owner = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or(SlugError::Empty)?;
        let name = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or(SlugError::Missing)?;
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
            },
            Self::TooManyParts => {
                write!(f, "slug has too many `/`; expected `<owner>/<name>`")
            },
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
        assert!(matches!(
            ForgeOp::parse_slug("homma"),
            Err(SlugError::Missing)
        ));
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
        let cli =
            Cli::try_parse_from(["homma", "-vv", "--output", "json", "-c", "alt.toml", "verify"])
                .unwrap();
        assert_eq!(cli.verbosity, 2);
        assert_eq!(cli.output, OutputFormat::Json);
        assert_eq!(
            cli.config.as_deref().map(|p| p.to_str().unwrap()),
            Some("alt.toml")
        );
        assert!(matches!(cli.command, Command::Verify {
            forge: false,
        }));
    }

    #[test]
    fn cli_parses_forge_show() {
        let cli =
            Cli::try_parse_from(["homma", "forge", "show", "github", "orgrinrt/homma"]).unwrap();
        match cli.command {
            Command::Forge {
                op:
                    ForgeOp::Show {
                        forge,
                        slug,
                    },
            } => {
                assert_eq!(forge, "github");
                assert_eq!(slug, "orgrinrt/homma");
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_migrate() {
        let cli = Cli::try_parse_from(["homma", "migrate", "notko", "--to", "codeberg"]).unwrap();
        match cli.command {
            Command::Migrate {
                repo,
                to,
                to_owner,
                to_org,
                source,
                dry_run,
            } => {
                assert_eq!(repo, "notko");
                assert_eq!(to, "codeberg");
                assert_eq!(to_owner, None);
                assert!(!to_org);
                assert_eq!(source, None);
                assert!(!dry_run);
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_migrate_with_overrides() {
        let cli = Cli::try_parse_from([
            "homma",
            "migrate",
            "notko",
            "--to",
            "codeberg",
            "--to-owner",
            "hiisi-digital",
            "--to-org",
            "--source",
            "github",
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Command::Migrate {
                repo,
                to,
                to_owner,
                to_org,
                source,
                dry_run,
            } => {
                assert_eq!(repo, "notko");
                assert_eq!(to, "codeberg");
                assert_eq!(to_owner.as_deref(), Some("hiisi-digital"));
                assert!(to_org);
                assert_eq!(source.as_deref(), Some("github"));
                assert!(dry_run);
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_archive_with_overrides() {
        let cli = Cli::try_parse_from([
            "homma", "archive", "notko", "--from", "github", "--owner", "orgrinrt",
        ])
        .unwrap();
        match cli.command {
            Command::Archive {
                repo,
                from,
                owner,
            } => {
                assert_eq!(repo, "notko");
                assert_eq!(from.as_deref(), Some("github"));
                assert_eq!(owner.as_deref(), Some("orgrinrt"));
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_rejects_unknown_subcommand() {
        let err = Cli::try_parse_from(["homma", "nope"]).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("unrecognized") || s.contains("invalid"),
            "got: {s}"
        );
    }
}

/// The roles, at the command line.
///
/// A separate enum from the registry's own so that the command surface can name
/// them without the vocabulary crate depending on clap.
#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
pub enum RoleArg {
    King,
    Hand,
    Expert,
    General,
}

impl From<RoleArg> for homma_api::Role {
    fn from(r: RoleArg) -> Self {
        match r {
            RoleArg::King => homma_api::Role::King,
            RoleArg::Hand => homma_api::Role::Hand,
            RoleArg::Expert => homma_api::Role::Expert,
            RoleArg::General => homma_api::Role::General,
        }
    }
}

/// `homma org` operations.
#[derive(Debug, Subcommand)]
pub enum OrgOp {
    /// List every identity, with where each stands.
    List,

    /// Add an identity to the registry.
    ///
    /// Written **mapped** by default: an entry records that a domain is taken
    /// before anybody is put on it. `--staffed`, with a workspace and a git
    /// identity, is what says somebody has been.
    Add {
        /// What everything else addresses it by. Becomes a directory name and a
        /// file name, so it is restricted to `[a-z0-9-_]`.
        handle: String,

        /// king, hand, expert or general.
        #[arg(long)]
        role: RoleArg,

        /// The short name a human uses.
        #[arg(long)]
        nickname: Option<String>,

        /// The full form, for when the short one is too familiar.
        #[arg(long)]
        full_name: Option<String>,

        /// What it is good at. A default for routing, never a partition.
        #[arg(long)]
        domain: Option<String>,

        /// Meant to have a workspace. Without this the entry is mapped.
        #[arg(long)]
        staffed: bool,

        #[arg(long)]
        git_name: Option<String>,

        #[arg(long)]
        git_email: Option<String>,

        /// Where its work happens.
        #[arg(long)]
        workspace: Option<String>,
    },

    /// Create an identity's workspace directories, memory link and definitions.
    ///
    /// Idempotent: running it again against a workspace that already has them
    /// changes nothing and does not clear what is remembered.
    Up {
        /// The handle to stand up.
        handle: String,

        /// The workspace root to stand it up in.
        ///
        /// Defaults to the directory holding the configuration file, resolved
        /// to an absolute path. Not the current directory: where somebody
        /// happens to be standing is not a statement about which workspace is
        /// being operated on.
        #[arg(long)]
        root: Option<PathBuf>,
    },
}
