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

use std::ffi::OsString;
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
    /// What state the workspace is in.
    ///
    /// Covers every population homma can report offline: the manifest and its
    /// forges, the agent surfaces including whether each repo's git hooks are
    /// wired, the shared tool configs, and the working tree. By default it
    /// shows only what is not in the state it should be in, because that is
    /// what the question is usually asking.
    ///
    /// The narrower verbs stay the place to see one population entirely,
    /// healthy entries included.
    Status {
        /// Print every population whole rather than only what is wrong.
        ///
        /// Human output only. The JSON document always carries every
        /// population in full, because a machine reading one with the healthy
        /// members dropped would be reading a lie about the population.
        #[arg(long)]
        full: bool,
    },

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
    /// Each member repo that has adopted mockspace renders its own
    /// `.claude/` and `.github/` via `cargo mock`, from the builtin rules and
    /// whatever `agent/` templates it adds. `homma agent` walks the
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

    /// `homma.local.toml`: what this clone of the workspace is for.
    ///
    /// Beside the manifest and never committed. `[instance]` is homma's,
    /// `[tools.<name>]` belongs to the workspace's own tools.
    Local {
        #[command(subcommand)]
        op: LocalOp,
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

    /// The personas a session can dispatch: what is authored, and generating
    /// them.
    ///
    /// One persona is one file, authored as `.shared/agents/<name>.md.tmpl` and
    /// generated into `.claude/agents/<name>.md`. The name it declares is its
    /// filename, or it is refused.
    Agents {
        #[command(subcommand)]
        op: AgentsOp,
    },

    /// The release: a gate run on the pushing machine, its record and commit
    /// status, and the merge, tag, changelog, forge release and registry
    /// publish that carry the working trunk onto the release line.
    Release {
        #[command(subcommand)]
        op: ReleaseOp,
    },

    /// The milestone: where the current one began, and what the range since
    /// holds: its review partitions, the stand-ins it added and the branches
    /// the trunk does not yet contain.
    Milestone {
        #[command(subcommand)]
        op: MilestoneOp,
    },

    /// The git hooks: one entrypoint per event in the repository's own hooks
    /// directory, or an entry registered with git-shook where that owns the
    /// hooks path, running the entries `[hooks]` in `homma.toml` and in the
    /// overlay files its `layers` key names, the release gate on `pre-push`
    /// among them whether or not the table is written.
    Hook {
        #[command(subcommand)]
        op: HookOp,
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

    /// Copy what the person said to an agent session into the capture store.
    ///
    /// Reads the session's transcript and writes every message they typed, and
    /// every question put to them with its options and their answer, as one
    /// capture file. Only what came after the last capture from that session is
    /// taken, bar a message the harness wrote twice across a compaction that cut
    /// the transcript, whose second copy is taken again. A run with nothing new
    /// writes nothing. The file is written and left for the caller to commit.
    Capture {
        /// The capture's title, which also names its file.
        #[arg(long)]
        title:    String,
        /// A session id, or a transcript's path. Defaults to the session the
        /// command runs inside, `CLAUDE_CODE_SESSION_ID`, and outside one to
        /// the newest transcript the agent harness keeps for this workspace.
        #[arg(long)]
        session:  Option<String>,
        /// Take nothing stamped at or before this instant (RFC 3339), for the
        /// first run on a session. The store's own watermark is a line in the
        /// transcript and applies as well; a message typed during a turn is
        /// stamped earlier than lines written before it, so a later run passing
        /// this loses those.
        #[arg(long)]
        since:    Option<jiff::Timestamp>,
        /// A path or repository the capture bears on, beyond those its words
        /// mention. Repeatable.
        #[arg(long = "bears-on")]
        bears_on: Vec<String>,
        /// The store to write into for this run. Defaults to `captures` under
        /// `[paths]` in the manifest, `.data/op-responses/` when it is absent.
        #[arg(long)]
        into:     Option<PathBuf>,
    },
    /// Take the answers and notes left on a page or a channel.
    ///
    /// `take` writes what is new as one capture in the capture store and attaches each answer to the
    /// row it answers, and prints nothing when nothing was new. `delegate` is what a workspace's
    /// mockspace hook runs, and `hook` places or removes that hook.
    Answers {
        #[command(subcommand)]
        op: AnswersOp,
    },
    /// The content repository's own tools.
    ///
    /// Each is a directory `tools/<name>/` with a `tool.toml`. `homma tools`
    /// lists them with their verbs and says which descriptors are broken; a tool
    /// with `promote = true` is also reached as `homma <name> <verb>`.
    Tools {
        #[command(subcommand)]
        op: Option<ToolsOp>,
    },
    /// Which tool, script, verb, skill or rule does a thing, found by words.
    ///
    /// Say what you want to do in your own words: `homma what land a branch`. The answer is the things
    /// that do it, each with the exact command and the words that matched, and what sits beside them.
    /// Names, tags and summaries are matched by stem, synonym, prefix and misspelling. A footer counts
    /// what could not be searched for want of a descriptor. Exits non-zero when nothing matches.
    What {
        /// What you want to do, in your own words.
        #[arg(required = true, num_args = 1..)]
        words:   Vec<String>,
        /// How many answers to give.
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..))]
        limit:   u32,
        /// Only things of this kind. Repeatable.
        #[arg(long = "kind", value_enum)]
        kinds:   Vec<crate::cmd::what::doc::Kind>,
        /// Leave out the mockspace commands, the one source that starts a process.
        #[arg(long)]
        no_mock: bool,
    },
    /// A promoted tool of the content repository, `homma <tool> <verb> ...`.
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

/// `tools` subcommands.
#[derive(Debug, Subcommand)]
pub enum ToolsOp {
    /// Every tool, its verbs, and the descriptors that cannot be used.
    List {},
    /// Print the descriptors that cannot be used, and exit non-zero while there are any.
    Check {},
    /// Run a tool by name, promoted or not, and whatever built-in holds the name.
    Run {
        /// The tool.
        name:  String,
        /// Its verb and the words it is given.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        words: Vec<OsString>,
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

    /// The shared tool configs a repo is meant to have.
    Config {
        #[command(subcommand)]
        op: ConfigOp,
    },
}

/// `repo config` subcommands.
///
/// Two verbs rather than one because the commit path needs a check that
/// cannot write. Placing a config turns a check on, and a check that was not
/// running has not been passing, so the consequences land wherever that tool
/// looks. That is somebody's to face deliberately rather than something a gate
/// does to them mid-commit.
#[derive(Debug, Subcommand)]
pub enum ConfigOp {
    /// Compare a repo against the shared configs and report. Writes nothing.
    ///
    /// Exits non-zero when a repo is missing a config that is required of it,
    /// which is what the commit path reads.
    Check {
        /// Single repo from `homma.toml`. Default: all.
        #[arg(long)]
        repo: Option<String>,
    },

    /// Place the shared configs a repo is missing.
    Init {
        /// Single repo from `homma.toml`. Default: all.
        #[arg(long)]
        repo: Option<String>,
    },
}

/// `agent` subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentOp {
    /// Discover per-repo mockspace agent-template state.
    ///
    /// For each member repo (or one when `--repo` is set), reports whether
    /// it carries a `mockspace.toml`, which is what adopting mockspace is,
    /// and for one that does, whether its mock directory, `.claude/`,
    /// `.github/instructions/` and the git hooks wiring are present.
    Status {
        /// Single repo from `homma.toml`. Default: all.
        #[arg(long)]
        repo: Option<String>,
    },
    /// Regenerate per-repo agent surfaces and aggregate them at the
    /// workspace level.
    ///
    /// Three stages:
    /// 1. Run `cargo mock` in each member repo with a `mockspace.toml`,
    ///    regenerating `.claude/rules/*.md` and `.claude/hooks/*.sh` from the
    ///    builtin rules and whatever `agent/` templates the repo adds under
    ///    its mock directory.
    /// 2. Compare each repo's shared tool configs against the shared copies
    ///    and place what is missing.
    /// 3. Aggregate per-repo rules and hooks into the workspace
    ///    `.claude/`, with each repo's rules and hooks scoped to only
    ///    activate when work touches that repo's tree.
    ///
    /// Repos without a `mockspace.toml` skip the first stage, not fail it.
    /// Each `--skip-*` flag leaves the other two stages running.
    /// `--skip-aggregate` also skips what follows the loop beside it:
    /// writing the commit gate, the declared hooks and the merge of the
    /// local settings.
    Regen {
        /// Single repo from `homma.toml`. Default: all repos.
        #[arg(long)]
        repo:              Option<String>,
        /// Keep going after a per-repo regen failure.
        #[arg(long)]
        continue_on_error: bool,
        /// Skip the per-repo `cargo mock` step; the shared tool configs
        /// and the workspace aggregation still run, against the
        /// already-rendered per-repo `.claude/`.
        #[arg(long)]
        skip_cargo_mock:   bool,
        /// Skip comparing each repo against the shared tool configs.
        #[arg(long)]
        skip_configs:      bool,
        /// Skip the workspace aggregation step, and with it the commit
        /// gate, the declared hooks and the settings merge; per-repo
        /// `cargo mock` and the shared tool configs still run.
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

/// `local` subcommands.
#[derive(Debug, Subcommand)]
pub enum LocalOp {
    /// Write the file with every key in it, and ignore it in `.gitignore`.
    ///
    /// Refuses to overwrite one that exists.
    Init {
        /// The body of work this clone is for.
        #[arg(long)]
        work: String,
    },
    /// Print the file. Exit 1 where there is none.
    Show,
    /// The stored configs by name, and which one is installed. Exit 1 where
    /// there is no store.
    ///
    /// The store is `.shared/local/` under the workspace root. A name is
    /// installed where the file's `instance.work` is that name.
    List,
    /// Put a stored config in place as this clone's `homma.local.toml`.
    ///
    /// A copy, which the clone then owns: what tools record goes into it and
    /// never back into the store. Where no file exists it is written. Where
    /// one exists for the same work it gains the keys it lacks and no key it
    /// has is changed, so running it again is safe. Where it is for another
    /// work, or is a link, it is refused unless `--replace` is given, which
    /// moves the old file aside to `homma.local.toml.replaced`.
    Install {
        /// The stored name, which is the work it is for.
        name:    String,
        /// Move a file for another work, or a link, aside and install over it.
        #[arg(long)]
        replace: bool,
    },
    /// Write one string at `instance.<key>` or `tools.<name>.<key>`.
    ///
    /// Keeps every comment, and refuses a value that would leave the file
    /// unable to load.
    Set {
        /// `instance.<key>` or `tools.<name>.<key>`.
        key:   String,
        /// The string to write.
        value: String,
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

/// `agents` subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentsOp {
    /// Every persona, and the sentence saying when to reach for it.
    List {},

    /// Generate the personas a session dispatches, from the authored templates.
    ///
    /// Each generated persona is rewritten whole, so editing one by hand loses
    /// the edit on the next run. A generated persona whose template is gone is
    /// named and left where it is.
    Render {},
}

/// `release` subcommands: the gate, the record it leaves, and the merge, tag,
/// changelog, forge release and registry publish that carry `dev` onto
/// `main`.
#[derive(Debug, Subcommand)]
pub enum ReleaseOp {
    /// The invariants a release rests on, by id, blocking ones first. Exits
    /// non-zero when any blocks.
    Check {
        /// The repository's directory name under the workspace root; the one
        /// the working directory is in when omitted.
        repo: Option<String>,
    },
    /// Run the gate on the checkout now, record the run, post its status.
    Gate {
        /// The repository's directory name; the working directory's when omitted.
        repo:     Option<String>,
        /// Refuse unless the checkout is at this sha, since the gate measures
        /// the tree it is given and nothing else.
        #[arg(long)]
        sha:      Option<String>,
        /// What the pre-push hook calls: reads the refs being pushed on stdin,
        /// gates the tip when it is among them, and exits non-zero on red so
        /// the push stops.
        #[arg(long, conflicts_with_all = ["sha", "post"])]
        hook:     bool,
        /// Post the status of an already recorded run on this sha again,
        /// without running anything.
        #[arg(long, conflicts_with = "sha")]
        post:     Option<String>,
        /// With `--post`: wait until the forge knows the commit before
        /// posting, up to ten minutes. What the pre-push hook leaves behind,
        /// since it measures a commit the push has not sent yet.
        #[arg(long = "await", requires = "post")]
        wait:     bool,
        /// What git hands a pre-push hook after its own arguments: the
        /// remote's name and url. Read by nothing here; the refs come on
        /// stdin. Hidden, since only the hook passes them.
        #[arg(trailing_var_arg = true, hide = true)]
        git_args: Vec<String>,
    },
    /// Print what a release at this level would do, and do nothing.
    Plan {
        /// The repository's directory name; the working directory's when omitted.
        repo:  Option<String>,
        /// `patch`, `minor` or `major`; never inferred.
        #[arg(long)]
        level: homma_api::Level,
    },
    /// The release: check, the recorded green run, the plan, then the bump,
    /// the merge and tag, the forge release, the publishes and the badges.
    Run {
        /// One repository, or every repository with something unreleased
        /// when omitted.
        repo:    Option<String>,
        /// `patch`, `minor` or `major`; never inferred.
        #[arg(long)]
        level:   homma_api::Level,
        /// Stop after printing the plan.
        #[arg(long)]
        dry_run: bool,
    },
    /// Rewrite the `badges` branch from the newest recorded run and the
    /// current version.
    Badges {
        repo: String,
    },
}

/// `milestone` subcommands: one record and four reports over a range.
#[derive(Debug, Subcommand)]
pub enum MilestoneOp {
    /// Print the sha of the current milestone, the last one marked.
    Base {
        /// The repository's directory name under the workspace root.
        repo: String,
    },
    /// Record a milestone in `.homma/milestones.toml`, committing nothing.
    Mark {
        /// The repository's directory name under the workspace root.
        repo: String,
        /// A commit the trunk contains.
        sha:  String,
        /// Also make the annotated tag `milestone/<n>` here, for where a push
        /// of tags works.
        #[arg(long)]
        tag:  bool,
    },
    /// The range split by crate and by document, with changed lines each.
    Partition {
        repo: String,
        base: String,
        head: String,
    },
    /// Every FIXME, relaxed assertion and marked stand-in the range added.
    StandIns {
        repo: String,
        base: String,
        head: String,
    },
    /// Every branch the trunk does not contain, with its changed lines, its
    /// conflicts against the trunk and its call.
    Branches {
        repo: String,
    },
}

/// `hook` subcommands.
#[derive(Debug, Subcommand)]
pub enum HookOp {
    /// Write one entrypoint per event the `[hooks]` table names into the
    /// repository's own hooks directory, or refuse and say why. Where the
    /// repository's hooks path is git-shook's, register a manifest generated
    /// from every layer with it instead, and run `git shook install --yes --only homma`,
    /// so the yes covers that manifest and no tracked `shook.toml`.
    Install {
        /// The repository's directory name under the workspace root.
        repo: String,
    },
    /// Which `homma.*.toml` files the hooks are read from, which are named and
    /// absent, and which are present and named by nothing, never read. With a
    /// repository, whether git reaches each of its events. Exits non-zero
    /// when anything needs looking at.
    Doctor {
        /// A repository's directory name under the workspace root.
        repo: Option<String>,
    },
    /// What an entrypoint calls: run the event's entries, in order, against
    /// what the event touched, and exit with the first refusal.
    Run {
        /// The git event, `pre-commit`, `pre-push`, `commit-msg` and the rest.
        event: String,
        /// What git hands the hook after its own name, passed on to every
        /// entry. Hidden, since only an entrypoint passes them.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
        args:  Vec<String>,
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
    fn the_gate_under_the_hook_takes_gits_remote_name_and_url_as_trailing_arguments() {
        // git runs `pre-push <remote name> <remote url>`, and the hook passes
        // both on; neither is a repo and neither is an error
        let cli = Cli::try_parse_from([
            "homma",
            "release",
            "gate",
            "--hook",
            "origin",
            "git@github.com:orgrinrt/notko.git",
        ])
        .unwrap();
        match cli.command {
            Command::Release {
                op:
                    ReleaseOp::Gate {
                        repo,
                        hook,
                        git_args,
                        ..
                    },
            } => {
                assert!(hook);
                assert_eq!(repo.as_deref(), Some("origin"));
                assert_eq!(git_args, vec![
                    "git@github.com:orgrinrt/notko.git".to_string()
                ]);
            },
            other => panic!("{other:?}"),
        }
        // and without the hook a bare name is still the repo
        let cli = Cli::try_parse_from(["homma", "release", "gate", "x"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Release {
                op: ReleaseOp::Gate { repo: Some(ref r), hook: false, .. }
            } if r == "x"
        ));
    }

    #[test]
    fn cli_parses_basic_invocation() {
        let cli = Cli::try_parse_from(["homma", "status"]).unwrap();
        assert!(matches!(cli.command, Command::Status {
            full: false,
        }));
        assert_eq!(cli.output, OutputFormat::Human);
        assert_eq!(cli.verbosity, 0);
    }

    #[test]
    fn status_full_is_off_unless_asked_for() {
        // The default is what nearly every invocation gets, and the whole
        // point of the default is that a healthy workspace stays short.
        let cli = Cli::try_parse_from(["homma", "status", "--full"]).unwrap();
        assert!(matches!(cli.command, Command::Status {
            full: true,
        }));
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
    fn cli_parses_repo_config_check_over_every_repo() {
        let cli = Cli::try_parse_from(["homma", "repo", "config", "check"]).unwrap();
        match cli.command {
            Command::Repo {
                op:
                    RepoOp::Config {
                        op:
                            ConfigOp::Check {
                                repo,
                            },
                    },
            } => assert_eq!(repo, None, "no --repo must mean every member, not none"),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_repo_config_check_for_one_repo() {
        // The form the commit gate runs, so it is worth pinning by name.
        let cli =
            Cli::try_parse_from(["homma", "repo", "config", "check", "--repo", "arvo"]).unwrap();
        match cli.command {
            Command::Repo {
                op:
                    RepoOp::Config {
                        op:
                            ConfigOp::Check {
                                repo,
                            },
                    },
            } => assert_eq!(repo.as_deref(), Some("arvo")),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_parses_repo_config_init() {
        // The form a refusal names as the fix. If this stops parsing, the
        // message tells somebody to run something that does not work.
        let cli =
            Cli::try_parse_from(["homma", "repo", "config", "init", "--repo", "arvo"]).unwrap();
        match cli.command {
            Command::Repo {
                op:
                    RepoOp::Config {
                        op:
                            ConfigOp::Init {
                                repo,
                            },
                    },
            } => assert_eq!(repo.as_deref(), Some("arvo")),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn cli_rejects_a_config_verb_that_does_not_exist() {
        // The control on the two above: a pass there could otherwise be clap
        // accepting anything after `config`.
        assert!(Cli::try_parse_from(["homma", "repo", "config", "place"]).is_err());
        assert!(Cli::try_parse_from(["homma", "repo", "config"]).is_err());
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
    fn an_unknown_subcommand_is_handed_to_the_tool_dispatch_which_refuses_it_by_name() {
        // clap no longer refuses it: a promoted tool of the content repository may be what it names. The
        // refusal, with the wording, is the dispatch's and is held in `tests/tools_dispatch.rs`.
        let cli = Cli::try_parse_from(["homma", "nope"]).unwrap();
        assert!(matches!(cli.command, Command::External(ref w) if w == &["nope"]));
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

/// `homma answers` operations.
#[derive(Debug, Subcommand)]
pub enum AnswersOp {
    /// Write what is new in a directory or channel of answer and note documents as one capture,
    /// and attach each answer to its row.
    #[command(group(clap::ArgGroup::new("source").required(true).args(["dir", "channel"])))]
    Take {
        /// A directory holding `answers/` and `notes/` of JSON documents.
        #[arg(long)]
        dir:     Option<PathBuf>,
        /// A channel, read through the workspace's `msg` tool.
        #[arg(long)]
        channel: Option<String>,
        /// Whose messages on the channel are taken.
        #[arg(long, default_value = "op")]
        from:    String,
        /// The store to write into for this run. Defaults to `captures` under `[paths]`.
        #[arg(long)]
        into:    Option<PathBuf>,
    },
    /// What mockspace's `registry-append` hook runs: takes words appended to a row's `answers`
    /// whole, and exits 3 for anything else so mockspace writes the row.
    Delegate {
        /// `<namespace>::<id>` of the row.
        target: String,
        field:  String,
        value:  String,
    },
    /// Place the hook mockspace consults before it writes a row, or take it away.
    Hook {
        #[arg(long)]
        remove: bool,
    },
}
