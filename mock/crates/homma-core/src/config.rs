//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma.toml` schema and parser.
//!
//! Workspace-level configuration for homma: workspace name, defaults, forge
//! profiles, and per-repo entries. Parsed from the workspace root's
//! `homma.toml` via [`Config::from_str`] or [`Config::from_path`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Parsed `homma.toml` contents.
///
/// No `Default` impl: a homma workspace without a name is meaningless, and
/// silently defaulting `workspace.name` to the empty string would poison
/// downstream code that reads it as a display label.
/// Construct via [`Config::parse`] / [`Config::from_path`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub workspace:    WorkspaceConfig,
    #[serde(default)]
    pub defaults:     Defaults,
    #[serde(default)]
    pub forges:       BTreeMap<String, ForgeConfig>,
    /// The member repositories, detected from the tree rather than parsed.
    ///
    /// Empty until [`Config::detect_members`] has run, which
    /// [`Config::from_path`] does at the load.
    ///
    /// `skip_deserializing` rather than `default`, so a manifest still
    /// carrying a `[repos]` table meets `deny_unknown_fields` and fails
    /// instead of being quietly half-read. Not `skip`, which would take the
    /// serialising half with it, and that half is what the template context
    /// reads: a document looping over the workspace's repositories would find
    /// none and render an empty list rather than an error.
    #[serde(skip_deserializing)]
    pub repos:        BTreeMap<String, RepoConfig>,
    /// The repository holding workspace metadata and content.
    ///
    /// Required by `homma_api::Workspace`, which parses the same file. Optional
    /// here because a workspace predating the registry has none, and because
    /// two parsers over one file must not deny each other's fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_repo: Option<String>,

    /// Where homma keeps things. Read by `homma_api::Workspace`; carried here so
    /// `deny_unknown_fields` does not reject it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<toml::Value>,

    /// The registry. Same reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<toml::Value>,

    /// Places homma may not write, beyond the ones it derives.
    ///
    /// Read here as well as by `homma_api::Workspace`, which parses the same
    /// file: this parser denies unknown fields, so a key one of them accepts and
    /// the other does not is a manifest neither can be sure of.
    #[serde(default)]
    pub deny: Vec<homma_api::DenyEntry>,

    /// Where forge credentials come from when no environment variable holds
    /// one. See [`AuthConfig`].
    #[serde(default)]
    pub auth: AuthConfig,

    /// The engine pin, which belongs to the launcher and not to this program.
    ///
    /// The launcher reads this same file to decide which engine to build and
    /// run, and it looks for exactly these five keys. They are declared here so
    /// `deny_unknown_fields` does not reject the file the launcher just used to
    /// find this binary, which is the same reason `paths` and `org` are above.
    ///
    /// Nothing here reads them. They are the launcher's answer to a question
    /// that was already settled by the time a command body runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homma_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homma_rev:     Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homma_branch:  Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homma_tag:     Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homma_git:     Option<String>,
}

impl Config {
    /// Parse `homma.toml` contents from a string.
    ///
    /// Named `parse` rather than `from_str` to avoid shadowing
    /// [`core::str::FromStr::from_str`]; a matching `FromStr` impl is also
    /// provided, so `"...".parse::<Config>()` works too.
    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        toml::from_str(s).map_err(ConfigError::Parse)
    }

    /// Parse `homma.toml` from a filesystem path.
    ///
    /// A relative `workspace.path` is resolved against the directory holding
    /// the config, not against the working directory. The config file sits at
    /// the workspace root by definition, so that is the one anchor that is
    /// right whatever directory homma was invoked from and wherever the
    /// workspace was spawned.
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let s = std::fs::read_to_string(path).map_err(|e| {
            ConfigError::Io {
                path:   path.to_path_buf(),
                source: e,
            }
        })?;
        let mut cfg = Self::parse(&s)?;
        // The config path is made absolute first, and that is the whole of
        // this. `Path::new("homma.toml").parent()` is `Some("")` rather than
        // `None`, so a fallback to `.` never fires for the spelling an operator
        // actually types, `-c homma.toml`, and every join below then leaves a
        // relative path behind. Everything anchored on it inherits that:
        // `resolve_local_path` gives up and returns `./<repo>`, the aggregated
        // hooks compare a relative root against the absolute path the host
        // supplies and never match, and a relative token-command program
        // anchors to nothing.
        //
        // The working directory is the right base rather than an invented one:
        // a relative path handed to a command means relative to where the
        // caller is.
        //
        // Computed once, before either use, because both want the same anchor
        // and the two arriving at different ones is the shape of a defect
        // nobody would look for.
        let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let absolute = if path.is_absolute() { path.to_path_buf() } else { here.join(path) };
        let beside = absolute.parent().unwrap_or(Path::new("."));
        if cfg.workspace.path.is_relative() {
            cfg.workspace.path = normalise(&beside.join(&cfg.workspace.path));
        }
        cfg.settle_token_commands(beside);
        cfg.settle_deny(beside);
        // The workspace root rather than the manifest's own directory. The two
        // are the same thing whenever `workspace.path` is left at `.`, which is
        // the ordinary case, and they are not when it is set. Detecting beside
        // the manifest while `resolve_local_path` anchors on the root would
        // hand every consumer a member whose path points at a directory that
        // was never looked in.
        let root = cfg.workspace.path.clone();
        cfg.detect_members(&root, &crate::repo::GixGit);
        Ok(cfg)
    }

    /// Fill [`Config::repos`] by walking `root` for member repositories.
    ///
    /// A member is a directory one level under the root whose `.git` is a
    /// directory. One level, because a repository nested inside a member is
    /// that member's business and the convention is that clones are root-level
    /// siblings. A `.git` that is a **file** points at another repository's
    /// object store, which makes it a worktree or a submodule, and a worktree
    /// of a member is not a second member.
    ///
    /// Separate from parsing on purpose. [`Config::parse`] takes a string and
    /// has no filesystem, so it cannot detect anything and must not pretend
    /// to: a `parse` whose result depended on the process's working directory
    /// would be the worst of both. A caller holding a config built from a
    /// string calls this with the root it means.
    ///
    /// Replaces whatever is there, so calling it twice with different roots
    /// gives the second root's answer rather than the union.
    pub fn detect_members<G: homma_api::Git>(&mut self, root: &Path, git: &G) {
        self.repos = BTreeMap::new();
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join(".git").is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let origin = self.read_origin_of(git, &path);
            self.repos.insert(name.to_string(), RepoConfig {
                forge:      origin.as_ref().and_then(|o| self.forge_at(&o.host)),
                owner:      origin.map(|o| o.owner),
                local_path: PathBuf::from(name),
            });
        }
    }

    /// The `origin` remote of a clone, through the git the caller supplied.
    ///
    /// Through the trait rather than a subprocess. A fork per directory under
    /// the root, on every config load, is the smaller half of it: the larger is
    /// that a machine without `git` on its path answers nothing and every
    /// member silently loses its forge, which reads as a workspace of clones
    /// that live nowhere.
    ///
    /// Best effort by design: a repository whose remote cannot be read is
    /// still a member, so the failure is an absent forge rather than an absent
    /// member.
    fn read_origin_of<G: homma_api::Git>(
        &self,
        git: &G,
        path: &Path,
    ) -> Option<crate::forge::url::RemoteOrigin> {
        // The trait takes an absolute path and says so in the type. A relative
        // root reaches here from a caller that built one, and canonicalising is
        // the answer rather than refusing: the directory exists, since it was
        // just read out of the tree.
        let absolute = path.canonicalize().ok()?;
        let absolute = homma_api::AbsPath::new(absolute).ok()?;
        let url = git.origin_url(&absolute).ok().flatten()?;
        crate::forge::url::read_origin(&url)
    }

    /// Which configured forge serves `host`, if any.
    ///
    /// Matched through the same [`host_of`] the composers use, so the
    /// direction that reads a remote and the direction that writes one agree
    /// by construction rather than by two spellings of one rule.
    ///
    /// [`host_of`]: crate::forge::url::host_of
    fn forge_at(&self, host: &str) -> Option<String> {
        self.forges
            .iter()
            .find(|(_, f)| crate::forge::url::host_of(&f.base_url) == host)
            .map(|(name, _)| name.clone())
    }

    /// Anchor every relative `deny` entry against the directory the manifest
    /// sits in, once, at the load.
    ///
    /// That directory is the anchor the entries are documented to have, and it
    /// is the one `workspace.path` and the token commands already take. Doing it
    /// here means nothing relative survives the load, so a later caller's own
    /// idea of the base cannot disagree with any of them. It could disagree:
    /// `workspace.path` may point away from the manifest, and the aggregation
    /// hands the workspace root down where the registry hands the manifest's own
    /// directory.
    ///
    /// A `~/` entry is left alone. Its anchor is the home rather than the
    /// manifest, resolving it is what `DenyEntry::resolve` does when a home is
    /// known, and doing that in two places is how the two come to disagree.
    ///
    /// Public for the same reason [`Config::settle_token_commands`] is: a caller
    /// that parsed a string rather than read a file is the only thing that knows
    /// which directory the text belongs to. Idempotent, since an entry made
    /// absolute here is absolute the second time through.
    pub fn settle_deny(&mut self, config_dir: &Path) {
        for entry in &mut self.deny {
            if entry.path.is_relative() && !entry.path.starts_with("~") {
                entry.path = normalise(&config_dir.join(&entry.path));
            }
        }
    }

    /// Inherit, substitute and anchor every forge's token command, once.
    ///
    /// Here rather than at the point of use so that nothing is spawned by a
    /// command that never asks a forge anything. `status` and `verify` without
    /// `--forge` are offline and stay offline, and the substitution is pure and
    /// therefore checkable without running any of it.
    ///
    /// Public so a caller that built a [`Config`] by parsing a string rather
    /// than reading a file can settle it against a directory of its choosing.
    /// Idempotent: substituting an argument list that holds no placeholder
    /// leaves it as it was.
    pub fn settle_token_commands(&mut self, config_dir: &Path) {
        let inherited = self.auth.token_cmd.clone();
        for (name, forge) in &mut self.forges {
            let Some(argv) = forge.token_cmd.take().or_else(|| inherited.clone()) else {
                continue;
            };
            let host = crate::forge::url::host_of(&forge.api_url).to_string();
            let mut argv: Vec<String> = argv
                .into_iter()
                .map(|a| a.replace("{forge}", name).replace("{host}", &host))
                .collect();
            if let Some(first) = argv.first_mut() {
                let p = Path::new(first.as_str());
                // A bare program name is left alone so `PATH` finds it, the way
                // it would if typed. Anything carrying a separator is a path,
                // and a relative one is relative to the workspace root rather
                // than to whatever directory homma was invoked from.
                if p.is_relative() && p.components().count() > 1 {
                    *first = normalise(&config_dir.join(p)).display().to_string();
                }
            }
            forge.token_cmd = Some(argv);
        }
    }

    /// Look up a repo by name.
    pub fn repo(&self, name: &str) -> Option<&RepoConfig> {
        self.repos.get(name)
    }

    /// Look up a forge profile by name.
    pub fn forge(&self, name: &str) -> Option<&ForgeConfig> {
        self.forges.get(name)
    }
}

impl std::str::FromStr for Config {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// `[workspace]` section. `name` is parse-required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default = "default_workspace_path")]
    pub path: PathBuf,
}

fn default_workspace_path() -> PathBuf {
    PathBuf::from(".")
}

/// Drop `.` components and collapse `x/..` pairs, so a path joined from a
/// config's own directory reads as the directory it names.
///
/// Purely lexical, and deliberately so: it must answer for a workspace that
/// has not been created yet, which `canonicalize` cannot. A `..` following
/// something that is a symlink is therefore resolved the way the shell prints
/// it rather than the way the kernel walks it, which is the accepted cost.
fn normalise(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir
                if out
                    .components()
                    .next_back()
                    .is_some_and(|c| matches!(c, std::path::Component::Normal(_))) =>
            {
                out.pop();
            },
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// `[defaults]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_license")]
    pub license:        String,
    #[serde(default = "default_public_branch")]
    pub public_branch:  String,
    #[serde(default = "default_working_branch")]
    pub working_branch: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            license:        default_license(),
            public_branch:  default_public_branch(),
            working_branch: default_working_branch(),
        }
    }
}

fn default_license() -> String {
    "MPL-2.0".into()
}
fn default_public_branch() -> String {
    "main".into()
}
fn default_working_branch() -> String {
    "dev".into()
}

/// `[forges.<name>]` entry. References a hosting service.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeConfig {
    pub kind:      ForgeKind,
    pub base_url:  String,
    pub api_url:   String,
    #[serde(default)]
    pub token_env: Option<String>,
    /// A command that prints this forge's token on stdout, asked when
    /// [`Self::token_env`] names no variable or that variable is empty.
    ///
    /// Inherited from [`AuthConfig::token_cmd`] when unset here, with the
    /// placeholders already substituted: by the time anything reads this it is
    /// a concrete argument list. Set it per forge for a credential a particular
    /// tool owns, which is the case for anything the operator logs into
    /// separately.
    #[serde(default)]
    pub token_cmd: Option<Vec<String>>,
}

/// `[auth]`: where a forge credential comes from when the environment holds
/// none.
///
/// A command rather than a file, because a credential lives wherever whatever
/// minted it put it: a keychain, a password manager, or a tool's own store. The
/// only thing they have in common is that something can be run to print one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// The default token command for every forge that names none of its own.
    ///
    /// An argument list, never a shell string. A shell would make a
    /// substituted placeholder into something that can be quoted out of, and
    /// nothing here needs a pipeline.
    ///
    /// Two placeholders are substituted in every element:
    ///
    /// - `{forge}`, the profile's own key in `[forges.*]`.
    /// - `{host}`, the host part of that profile's `api_url`.
    ///
    /// A first element that is a relative path containing a separator is
    /// resolved against the directory holding `homma.toml`, which is the
    /// workspace root. A bare program name is left alone, so it is found on
    /// `PATH` the way it would be if typed.
    #[serde(default)]
    pub token_cmd: Option<Vec<String>>,
}

/// Hosting service type. Drives client selection in [crate::forge].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeKind {
    Github,
    Forgejo,
}

/// One member repository, as detected rather than as declared.
///
/// Produced by [`Config::detect_members`] from the tree, never parsed. A
/// workspace that renames a repository, clones a new one or drops one is
/// correct the moment the tree is, and there is nothing left to disagree with
/// it. The list this replaced spent a month naming a crate that had been
/// renamed, and every reference in the workspace followed the list.
#[derive(Debug, Clone, Serialize)]
pub struct RepoConfig {
    /// Which key in [`Config::forges`] this member's remote host matched.
    ///
    /// `None` where the clone has no `origin`, where its remote is a local
    /// path, or where its host matches no configured forge. All three are
    /// ordinary and none of them stops the directory being a member: what a
    /// repository is does not depend on anybody having a profile for its host.
    /// Never guessed, because a guess here decides where a push lands.
    pub forge:      Option<String>,
    /// The namespace the remote puts it in, on the same terms as `forge`.
    pub owner:      Option<String>,
    /// The directory name, relative to the workspace root.
    pub local_path: PathBuf,
}

/// Parse / IO error surfaced by [`Config::from_str`] and [`Config::from_path`].
#[derive(Debug)]
pub enum ConfigError {
    Parse(toml::de::Error),
    Io {
        path:   PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "homma.toml parse error: {e}"),
            Self::Io {
                path,
                source,
            } => {
                write!(f, "failed to read {}: {source}", path.display())
            },
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Io {
                source,
                ..
            } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests;
