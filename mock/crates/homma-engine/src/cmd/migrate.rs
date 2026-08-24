//! `homma migrate <repo> --to <forge>`: replicate a repo to a destination forge.
//!
//! The command runs in five phases:
//!
//! 1. Resolve config: locate the named repo and the source/destination forge
//!    profiles.
//! 2. Read source: `Forge::fetch_repo` on the source forge.
//! 3. Pre-flight destination: `Forge::repo_exists` on the destination to
//!    avoid clobber.
//! 4. Create destination: `Forge::create_repo` with description, visibility,
//!    and default-branch replicated from source.
//! 5. Mirror clone + push: `GixRepo::mirror_into` to a tempdir, then
//!    `git push --mirror` to the destination. Auth, when present, flows
//!    through `GIT_CONFIG_COUNT` env vars so the token never lands in
//!    process argv.
//!
//! Topics replication and the post-push `default_branch` PATCH that the
//! `Forge` trait advertises for GitHub destinations are out of scope here.
//! They land alongside the sanity playground (#456) where a real Codeberg
//! destination exercises the full round-trip.
//!
//! Source-side archival is the separate `homma archive` step (see
//! [`super::archive`]).
//!
//! ## Partial-failure semantics
//!
//! If `create_repo` succeeds but the mirror push fails, the destination is
//! left as an empty repo and the error propagates. This is deliberate. An
//! automatic teardown step introduces its own failure modes (what if the
//! delete itself fails? do we report a half-created repo as a clean
//! failure?) and the operator can dispose of the empty destination
//! manually before retrying. Future agents: do not add a cleanup-on-push-
//! failure path without weighing this tradeoff.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use homma_core::forge::url as forge_url;
use homma_core::{
    Config,
    CreateRepoSpec,
    ForgeConfig,
    ForgeKind,
    GixRepo,
    MirrorOpts,
    RepoMetadata,
    Visibility,
};
use serde::Serialize;

use crate::cli::OutputFormat;
use crate::cmd::Outcome;
use crate::cmd::forge::{client_from_config, resolve_forge};
use crate::output::{HumanRender, emit};

/// Optional overrides from the CLI to the migrate body.
#[derive(Debug, Clone, Copy, Default)]
pub struct Opts<'a> {
    /// Destination owner. `None` reuses the source owner.
    pub to_owner: Option<&'a str>,
    /// Destination owner is an org namespace, not a user. Drives
    /// `CreateRepoSpec::in_org()`.
    pub to_org:   bool,
    /// Source forge profile override. `None` reads `[repos.<repo>].forge`.
    pub source:   Option<&'a str>,
    /// Plan-only: emit the plan and exit without contacting the destination.
    pub dry_run:  bool,
}

/// Top-level command result.
#[derive(Debug, Serialize)]
pub struct MigrateReport {
    pub plan:   MigratePlan,
    pub result: MigrateResult,
}

/// The replication plan, computed from source metadata before any
/// destination contact or push.
#[derive(Debug, Serialize)]
pub struct MigratePlan {
    pub source_forge:     String,
    pub source_owner:     String,
    pub source_name:      String,
    pub source_clone_url: String,
    pub dest_forge:       String,
    pub dest_owner:       String,
    pub dest_name:        String,
    pub default_branch:   String,
    pub visibility:       String,
    pub description:      Option<String>,
    pub topic_count:      usize,
}

/// What the command actually did.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MigrateResult {
    /// Plan emitted; no destination contact, no push.
    DryRun,
    /// Destination created and mirror pushed.
    Migrated {
        /// HTTPS clone URL as the destination forge reports it.
        dest_clone_url: String,
    },
}

pub fn run(
    cfg: &Config,
    repo_name: &str,
    to_forge_name: &str,
    opts: Opts<'_>,
    format: OutputFormat,
) -> Result<Outcome> {
    let repo_cfg = cfg
        .repo(repo_name)
        .ok_or_else(|| anyhow!("repo `{repo_name}` not declared in [repos.*]"))?;
    let source_forge_name = opts.source.unwrap_or(&repo_cfg.forge);
    let source_cfg = resolve_forge(cfg, source_forge_name)?;
    let dest_cfg = resolve_forge(cfg, to_forge_name)?;

    let src_owner = repo_cfg.owner.as_str();
    let src_name = repo_name;
    let dst_owner = opts.to_owner.unwrap_or(src_owner);
    let dst_name = repo_name;

    let source = client_from_config(source_cfg);
    let meta = source
        .fetch_repo(src_owner, src_name)
        .with_context(|| format!("reading {src_owner}/{src_name} from {source_forge_name}"))?;

    let plan = build_plan(
        source_forge_name,
        to_forge_name,
        src_owner,
        src_name,
        dst_owner,
        dst_name,
        &meta,
    );

    if opts.dry_run {
        emit(
            &MigrateReport {
                plan,
                result: MigrateResult::DryRun,
            },
            format,
        )?;
        return Ok(Outcome::Ok);
    }

    let dest = client_from_config(dest_cfg);
    if dest
        .repo_exists(dst_owner, dst_name)
        .with_context(|| format!("checking {dst_owner}/{dst_name} on {to_forge_name}"))?
    {
        bail!(
            "destination {dst_owner}/{dst_name} already exists on {to_forge_name}; \
             aborting before clobber",
        );
    }

    let spec = build_create_spec(dst_name, &meta, opts.to_org);
    let created = dest
        .create_repo(dst_owner, &spec)
        .with_context(|| format!("creating {dst_owner}/{dst_name} on {to_forge_name}"))?;

    let tempdir = tempfile::tempdir().context("creating tempdir for mirror clone")?;
    let mirror_dir = tempdir.path().join("mirror.git");
    GixRepo::mirror_into(&meta.clone_url_https, &mirror_dir, MirrorOpts::default())
        .with_context(|| format!("mirror-cloning {}", meta.clone_url_https))?;

    let dest_push_url = forge_url::clone_https(dest_cfg, dst_owner, dst_name);
    let token = read_token(dest_cfg);
    push_mirror(&mirror_dir, &dest_push_url, dest_cfg.kind, token.as_deref())
        .with_context(|| format!("pushing mirror to {dest_push_url}"))?;

    emit(
        &MigrateReport {
            plan,
            result: MigrateResult::Migrated {
                dest_clone_url: created.clone_url_https,
            },
        },
        format,
    )?;
    Ok(Outcome::Ok)
}

fn build_plan(
    source_forge: &str,
    dest_forge: &str,
    src_owner: &str,
    src_name: &str,
    dst_owner: &str,
    dst_name: &str,
    meta: &RepoMetadata,
) -> MigratePlan {
    MigratePlan {
        source_forge:     source_forge.into(),
        source_owner:     src_owner.into(),
        source_name:      src_name.into(),
        source_clone_url: meta.clone_url_https.clone(),
        dest_forge:       dest_forge.into(),
        dest_owner:       dst_owner.into(),
        dest_name:        dst_name.into(),
        default_branch:   meta.default_branch.clone(),
        visibility:       visibility_str(meta.visibility).into(),
        description:      meta.description.clone(),
        topic_count:      meta.topics.len(),
    }
}

fn build_create_spec(dst_name: &str, meta: &RepoMetadata, to_org: bool) -> CreateRepoSpec {
    let spec = CreateRepoSpec::new(dst_name).replicate_from(meta);
    if to_org { spec.in_org() } else { spec }
}

/// Read the token for a forge via its `token_env` name, if set.
fn read_token(forge: &ForgeConfig) -> Option<String> {
    forge
        .token_env
        .as_deref()
        .and_then(|name| std::env::var(name).ok())
}

/// Run `git push --mirror` from `mirror_dir` to `dest_url`.
///
/// Auth, when present, is wired through `GIT_CONFIG_COUNT` env vars so the
/// token does not appear in the subprocess's argv (visible to other users
/// on shared hosts via `ps` or `/proc/<pid>/cmdline`). The header value is
/// still in the subprocess address space, which is unavoidable for any
/// subprocess auth scheme.
fn push_mirror(
    mirror_dir: &Path,
    dest_url: &str,
    kind: ForgeKind,
    token: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.current_dir(mirror_dir);
    if let Some(t) = token {
        let header = match kind {
            ForgeKind::Github => format!("AUTHORIZATION: bearer {t}"),
            ForgeKind::Forgejo => format!("Authorization: token {t}"),
        };
        cmd.env("GIT_CONFIG_COUNT", "1");
        cmd.env("GIT_CONFIG_KEY_0", "http.extraheader");
        cmd.env("GIT_CONFIG_VALUE_0", header);
    }
    cmd.arg("push").arg("--mirror").arg(dest_url);
    let output = cmd.output().context("invoking git push")?;
    if !output.status.success() {
        let stderr = scrub_token(&String::from_utf8_lossy(&output.stderr), token);
        bail!(
            "git push --mirror failed (exit {}): {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(())
}

/// Replace any literal occurrence of `token` in `s` with a placeholder.
///
/// Paranoid scrub. Current paths never echo the token (it flows via env
/// vars, not argv), but the diagnostic surface is kept token-free
/// defensively in case a future git release prints config values on error.
///
/// Skips short tokens (under 16 chars) to avoid false-positive redaction
/// of unrelated short substrings. Real PATs are 40+ high-entropy chars;
/// anything shorter than 16 is most likely a misconfigured `token_env`
/// pointing at a placeholder, not a real secret.
fn scrub_token(s: &str, token: Option<&str>) -> String {
    const MIN_REDACT_LEN: usize = 16;
    match token {
        Some(t) if t.len() >= MIN_REDACT_LEN => s.replace(t, "<redacted>"),
        _ => s.to_string(),
    }
}

fn visibility_str(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Private => "private",
        Visibility::Internal => "internal",
    }
}

impl HumanRender for MigrateReport {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let p = &self.plan;
        writeln!(
            out,
            "migrate {}/{} ({}) -> {}/{} ({})",
            p.source_owner, p.source_name, p.source_forge, p.dest_owner, p.dest_name, p.dest_forge,
        )?;
        writeln!(
            out,
            "  default_branch={}  visibility={}  topics={}",
            p.default_branch, p.visibility, p.topic_count,
        )?;
        if let Some(desc) = &p.description {
            writeln!(out, "  description: {desc}")?;
        }
        writeln!(out, "  source clone: {}", p.source_clone_url)?;
        match &self.result {
            MigrateResult::DryRun => writeln!(out, "  dry run: no destination contact"),
            MigrateResult::Migrated {
                dest_clone_url,
            } => {
                writeln!(out, "  pushed to: {dest_clone_url}")
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use homma_core::forge::OwnerKind;

    use super::*;

    fn sample_meta() -> RepoMetadata {
        RepoMetadata {
            owner:           "orgrinrt".into(),
            name:            "notko".into(),
            description:     Some("foundation primitives".into()),
            default_branch:  "dev".into(),
            visibility:      Visibility::Public,
            topics:          vec!["rust".into(), "no-std".into()],
            archived:        false,
            clone_url_https: "https://github.com/orgrinrt/notko.git".into(),
        }
    }

    #[test]
    fn scrub_replaces_realistic_token() {
        // 40-char hex string, the shape of a real GitHub PAT.
        let token = "ghp_1234567890abcdef1234567890abcdef1234567";
        let s = format!("fatal: bad credentials ({token} invalid)");
        let out = scrub_token(&s, Some(token));
        assert!(out.contains("<redacted>"));
        assert!(!out.contains(token));
    }

    #[test]
    fn scrub_no_token_passes_through() {
        let s = "fatal: connection refused";
        assert_eq!(scrub_token(s, None), s);
    }

    #[test]
    fn scrub_empty_token_passes_through() {
        let s = "fatal: connection refused";
        assert_eq!(scrub_token(s, Some("")), s);
    }

    #[test]
    fn scrub_short_token_does_not_redact() {
        // A 6-char "token" is below the min-redact length; without the guard
        // it would false-positive against unrelated short substrings.
        let s = "fatal: 401 (token abc123 invalid)";
        let out = scrub_token(s, Some("abc123"));
        assert_eq!(out, s);
        assert!(out.contains("abc123"));
    }

    #[test]
    fn build_create_spec_replicates_metadata() {
        let meta = sample_meta();
        let spec = build_create_spec("notko", &meta, false);
        assert_eq!(spec.name, "notko");
        assert_eq!(spec.description.as_deref(), Some("foundation primitives"));
        assert!(matches!(spec.visibility, Visibility::Public));
        assert_eq!(spec.default_branch.as_deref(), Some("dev"));
        assert!(!spec.auto_init);
        assert!(matches!(spec.owner_kind, OwnerKind::User));
    }

    #[test]
    fn build_create_spec_marks_org_when_requested() {
        let meta = sample_meta();
        let spec = build_create_spec("notko", &meta, true);
        assert!(matches!(spec.owner_kind, OwnerKind::Org));
    }

    #[test]
    fn build_plan_populates_fields() {
        let meta = sample_meta();
        let plan = build_plan(
            "github",
            "codeberg",
            "orgrinrt",
            "notko",
            "hiisi-digital",
            "notko",
            &meta,
        );
        assert_eq!(plan.source_forge, "github");
        assert_eq!(plan.dest_forge, "codeberg");
        assert_eq!(plan.source_owner, "orgrinrt");
        assert_eq!(plan.dest_owner, "hiisi-digital");
        assert_eq!(plan.default_branch, "dev");
        assert_eq!(plan.visibility, "public");
        assert_eq!(plan.topic_count, 2);
        assert_eq!(
            plan.source_clone_url,
            "https://github.com/orgrinrt/notko.git"
        );
    }

    #[test]
    fn visibility_str_round_trip() {
        assert_eq!(visibility_str(Visibility::Public), "public");
        assert_eq!(visibility_str(Visibility::Private), "private");
        assert_eq!(visibility_str(Visibility::Internal), "internal");
    }

    /// Exercise the `git push --mirror` subprocess against a local file://
    /// destination. Covers argv shape, current_dir, and the no-token path.
    /// The HTTP `extraheader` auth path is not exercised (file:// has no
    /// transport); that lands alongside the sanity playground (#456) where
    /// a real HTTPS push is exercisable.
    #[test]
    fn push_mirror_against_local_file_dest_succeeds() {
        let tempdir = tempfile::tempdir().unwrap();
        let src_path = tempdir.path().join("src");
        let mirror_path = tempdir.path().join("mirror.git");
        let dest_path = tempdir.path().join("dest.git");

        // Source: init + commit one file.
        let init = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(tempdir.path())
            .arg(&src_path)
            .output()
            .unwrap();
        assert!(init.status.success(), "git init source failed");
        std::fs::write(src_path.join("README"), "hello\n").unwrap();
        // Configure local identity so the commit can be created in CI envs
        // that lack a global user.name / user.email.
        for (k, v) in [("user.email", "test@example.test"), ("user.name", "Test")] {
            Command::new("git")
                .args(["config", k, v])
                .current_dir(&src_path)
                .status()
                .unwrap();
        }
        Command::new("git")
            .args(["add", "README"])
            .current_dir(&src_path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "initial"])
            .current_dir(&src_path)
            .status()
            .unwrap();

        // Mirror clone source → mirror.git.
        GixRepo::mirror_into(
            src_path.to_str().unwrap(),
            &mirror_path,
            MirrorOpts::default(),
        )
        .expect("mirror_into source");

        // Destination: empty bare repo.
        let init_dest = Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&dest_path)
            .output()
            .unwrap();
        assert!(init_dest.status.success(), "git init --bare dest failed");

        // Push the mirror to the dest via file://.
        let dest_url = format!("file://{}", dest_path.display());
        push_mirror(&mirror_path, &dest_url, ForgeKind::Forgejo, None)
            .expect("push_mirror to file:// dest");

        // The dest should now carry refs/heads/main.
        let show_ref = Command::new("git")
            .args(["show-ref"])
            .current_dir(&dest_path)
            .output()
            .unwrap();
        let refs = String::from_utf8_lossy(&show_ref.stdout);
        assert!(
            refs.contains("refs/heads/main"),
            "dest missing refs/heads/main; got: {refs}",
        );
    }

    #[test]
    fn push_mirror_against_nonexistent_dest_fails() {
        let tempdir = tempfile::tempdir().unwrap();
        let mirror_path = tempdir.path().join("mirror.git");
        // Make a bare repo to act as the "mirror"; push will fail because
        // the dest does not exist.
        Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&mirror_path)
            .status()
            .unwrap();
        let bogus_url = format!("file://{}/does-not-exist", tempdir.path().display());
        let err = push_mirror(&mirror_path, &bogus_url, ForgeKind::Forgejo, None)
            .expect_err("push to nonexistent dest must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("git push --mirror failed"), "msg: {msg}");
    }
}
