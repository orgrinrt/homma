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
    #[serde(default)]
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
        Ok(cfg)
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

/// `[repos.<name>]` entry. One row per workspace repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoConfig {
    /// References a key in [`Config::forges`].
    pub forge:          String,
    pub owner:          String,
    pub local_path:     PathBuf,
    /// Overrides [`Defaults::public_branch`] when set.
    #[serde(default)]
    pub public_branch:  Option<String>,
    /// Overrides [`Defaults::working_branch`] when set.
    #[serde(default)]
    pub working_branch: Option<String>,
}

impl RepoConfig {
    /// Per-repo public branch override, falling back to `defaults.public_branch`.
    pub fn resolved_public_branch<'a>(&'a self, defaults: &'a Defaults) -> &'a str {
        self.public_branch
            .as_deref()
            .unwrap_or(&defaults.public_branch)
    }

    /// Per-repo working branch override, falling back to `defaults.working_branch`.
    pub fn resolved_working_branch<'a>(&'a self, defaults: &'a Defaults) -> &'a str {
        self.working_branch
            .as_deref()
            .unwrap_or(&defaults.working_branch)
    }
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
mod token_command_tests {
    use super::*;

    fn cfg(body: &str) -> Config {
        let mut c = Config::parse(body).unwrap();
        c.settle_token_commands(Path::new("/ws"));
        c
    }

    const TWO_FORGES: &str = r#"
[workspace]
name = "w"
[auth]
token_cmd = [".shared/scripts/release/auth", "token", "{forge}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
"#;

    #[test]
    fn one_line_serves_every_forge_because_the_placeholder_carries_the_name() {
        // The whole point of the default: the operator writes it once and each
        // profile asks about itself. A fixture with one forge cannot tell a
        // working substitution from a constant, so there are two.
        let c = cfg(TWO_FORGES);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap()[2], "github");
        assert_eq!(
            c.forges["codeberg"].token_cmd.as_ref().unwrap()[2],
            "codeberg"
        );
    }

    #[test]
    fn a_relative_program_path_is_anchored_to_the_workspace_root() {
        // Not to the working directory. `homma` is meant to run from inside a
        // member clone, where a path relative to cwd names nothing.
        let c = cfg(TWO_FORGES);
        assert_eq!(
            c.forges["github"].token_cmd.as_ref().unwrap()[0],
            "/ws/.shared/scripts/release/auth"
        );
    }

    #[test]
    fn a_bare_program_name_is_left_for_path_to_find() {
        // The control on the anchoring above: `gh` must stay `gh`, or the one
        // case that needs no configuration at all stops working.
        let c = cfg(r#"
[workspace]
name = "w"
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_cmd = ["gh", "auth", "token"]
"#);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap(), &[
            "gh", "auth", "token"
        ]);
    }

    #[test]
    fn a_forges_own_command_is_not_replaced_by_the_default() {
        let c = cfg(r#"
[workspace]
name = "w"
[auth]
token_cmd = ["shared", "{forge}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_cmd = ["gh", "auth", "token"]
[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
"#);
        assert_eq!(c.forges["github"].token_cmd.as_ref().unwrap(), &[
            "gh", "auth", "token"
        ]);
        // and the other one still inherits, which is what makes this a test
        // about precedence rather than about the default never applying
        assert_eq!(c.forges["codeberg"].token_cmd.as_ref().unwrap(), &[
            "shared", "codeberg"
        ]);
    }

    #[test]
    fn the_host_placeholder_is_the_api_host_and_not_the_public_one() {
        // They differ on GitHub, which is the case worth pinning: `github.com`
        // against `api.github.com`.
        let c = cfg(r#"
[workspace]
name = "w"
[auth]
token_cmd = ["t", "{host}"]
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
"#);
        assert_eq!(
            c.forges["github"].token_cmd.as_ref().unwrap()[1],
            "api.github.com"
        );
    }

    #[test]
    fn a_manifest_naming_no_command_anywhere_gets_none() {
        // The control on all of the above: nothing is invented for a manifest
        // that asked for nothing, so an operator who never opts in never has a
        // subprocess run on their behalf.
        let c = cfg(r#"
[workspace]
name = "w"
[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_env = "SOMETHING"
"#);
        assert!(c.forges["github"].token_cmd.is_none());
    }

    #[test]
    fn settling_twice_changes_nothing() {
        // `from_path` settles once, and a caller that parsed a string may
        // settle again. A second pass must not re-anchor an already absolute
        // path or substitute into a name that legitimately contains braces.
        let mut c = Config::parse(TWO_FORGES).unwrap();
        c.settle_token_commands(Path::new("/ws"));
        let once = c.forges["github"].token_cmd.clone();
        c.settle_token_commands(Path::new("/elsewhere"));
        assert_eq!(c.forges["github"].token_cmd, once);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_workspace_path_resolves_beside_the_config_not_beside_the_caller() {
        // The failure this exists to stop: the tracked `homma.toml` used to
        // carry an absolute path to one particular clone, so every repo lookup
        // from any other workspace resolved into that one, and the configs
        // stage would have written files into it.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("kamu-canon");
        std::fs::create_dir_all(&ws).unwrap();
        let at = ws.join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\n").unwrap();

        let cfg = Config::from_path(&at).unwrap();
        assert_eq!(
            cfg.workspace.path, ws,
            "the default did not anchor on the config"
        );
        assert!(cfg.workspace.path.is_absolute());
    }

    #[test]
    fn a_relative_config_path_still_yields_an_absolute_workspace_path() {
        // The half of the matrix the sibling test above does not reach: it
        // passes an absolute config path, so it never exercises the anchoring
        // it asserts. Every path in the program hangs off this one, and a
        // relative result is what made the aggregated hooks inert.
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("kamu-canon");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();

        let relative = pathdiff_from(&std::env::current_dir().unwrap(), &ws.join("homma.toml"));
        let cfg = Config::from_path(&relative).unwrap();
        assert!(
            cfg.workspace.path.is_absolute(),
            "a relative config path left the workspace relative: {}",
            cfg.workspace.path.display()
        );
        assert_eq!(cfg.workspace.path, normalise(&ws));
    }

    #[test]
    fn a_bare_filename_config_path_is_the_case_the_fallback_never_covered() {
        // Named so a later reader does not restore the `unwrap_or(".")` as
        // sufficient. `Path::new("homma.toml").parent()` is `Some("")`, not
        // `None`, so that fallback is unreachable for the one spelling that
        // needs it.
        assert_eq!(Path::new("homma.toml").parent(), Some(Path::new("")));
        // And the control: the fallback does fire for a path with no filename
        // at all, which is the case it was written for.
        assert_eq!(Path::new("").parent(), None);
    }

    #[test]
    fn an_absolute_config_path_is_unaffected_by_the_working_directory() {
        // The control on the change: absolutising the config path must not
        // move a config that already named itself absolutely.
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\npath = \"repos\"\n").unwrap();
        assert_eq!(
            Config::from_path(&at).unwrap().workspace.path,
            normalise(&dir.path().join("repos"))
        );
    }

    /// A path to `target` expressed relative to `base`, for the one test that
    /// needs a relative config path and cannot change the working directory
    /// without racing every other test in the binary.
    fn pathdiff_from(base: &Path, target: &Path) -> PathBuf {
        let base = normalise(base);
        let target = normalise(target);
        let mut up = PathBuf::new();
        let mut probe = base.as_path();
        loop {
            if let Ok(rest) = target.strip_prefix(probe) {
                return up.join(rest);
            }
            match probe.parent() {
                Some(p) => {
                    up.push("..");
                    probe = p;
                },
                None => return target,
            }
        }
    }

    #[test]
    fn an_absolute_workspace_path_is_left_exactly_as_written() {
        // The control: naming a path explicitly still means that path. The
        // resolution is for the relative case only.
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(
            &at,
            "[workspace]\nname = \"w\"\npath = \"/somewhere/else\"\n",
        )
        .unwrap();
        assert_eq!(
            Config::from_path(&at).unwrap().workspace.path,
            PathBuf::from("/somewhere/else")
        );
    }

    #[test]
    fn a_relative_path_that_climbs_lands_where_it_reads() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("a").join("b");
        std::fs::create_dir_all(&inner).unwrap();
        let at = inner.join("homma.toml");
        std::fs::write(&at, "[workspace]\nname = \"w\"\npath = \"../..\"\n").unwrap();
        assert_eq!(Config::from_path(&at).unwrap().workspace.path, dir.path());
    }

    #[test]
    fn parsing_a_string_leaves_the_path_alone_because_there_is_nothing_to_anchor_on() {
        // `parse` has no file, so it cannot resolve, and inventing the working
        // directory as an anchor would be the guess this whole change removes.
        let cfg = Config::parse("[workspace]\nname = \"w\"\npath = \"repos\"\n").unwrap();
        assert_eq!(cfg.workspace.path, PathBuf::from("repos"));
    }

    #[test]
    fn normalising_a_path_keeps_what_it_names() {
        assert_eq!(normalise(Path::new("/a/./b")), PathBuf::from("/a/b"));
        assert_eq!(normalise(Path::new("/a/b/..")), PathBuf::from("/a"));
        assert_eq!(normalise(Path::new("/a/b/../..")), PathBuf::from("/"));
        assert_eq!(normalise(Path::new("a/b/../c")), PathBuf::from("a/c"));
        // a leading climb has nothing to cancel against and is kept, rather
        // than silently becoming the relative root
        assert_eq!(normalise(Path::new("../a")), PathBuf::from("../a"));
        assert_eq!(normalise(Path::new("../../a")), PathBuf::from("../../a"));
        // and a path that cancels to nothing is still a path
        assert_eq!(normalise(Path::new("a/..")), PathBuf::from("."));
        assert_eq!(normalise(Path::new(".")), PathBuf::from("."));
    }
}

#[cfg(test)]
mod deny_anchor_tests {
    use super::*;

    /// Write a manifest into a fresh directory and load it the way a run does.
    ///
    /// Through the file rather than through `parse`, because the anchoring is
    /// what `from_path` adds and a string has no directory to be anchored to.
    fn loaded(body: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let at = dir.path().join("homma.toml");
        std::fs::write(&at, body).unwrap();
        let cfg = Config::from_path(&at).unwrap();
        (dir, cfg)
    }

    #[test]
    fn a_relative_entry_is_anchored_to_the_manifest_rather_than_the_caller() {
        let (dir, cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
"#,
        );
        assert_eq!(cfg.deny[0].path, dir.path().join("scratch"));
        // The control: the working directory is a different place, and an entry
        // anchored there would name it instead.
        assert_ne!(cfg.deny[0].path, std::env::current_dir().unwrap().join("scratch"));
    }

    #[test]
    fn the_anchor_holds_when_the_workspace_points_somewhere_else() {
        // The case the two anchors diverged on. The registry resolved a relative
        // entry against the manifest's directory and the aggregation resolved it
        // against the workspace root, so one manifest denied two different
        // places depending on which command read it.
        let (dir, cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
path = "elsewhere"
"#,
        );
        assert_eq!(cfg.deny[0].path, dir.path().join("scratch"));
        assert_ne!(cfg.deny[0].path, cfg.workspace.path.join("scratch"));
    }

    #[test]
    fn a_home_entry_and_an_absolute_one_come_through_untouched() {
        // `~/` belongs to the home rather than the manifest, and resolving it
        // here as well as in `DenyEntry::resolve` is how the two come to
        // disagree. An absolute entry names its place already.
        let (_dir, cfg) = loaded(
            r#"
deny = ["~/work/someone-elses", "/var/tmp/nope"]
[workspace]
name = "w"
"#,
        );
        assert_eq!(cfg.deny[0].path, Path::new("~/work/someone-elses"));
        assert_eq!(cfg.deny[1].path, Path::new("/var/tmp/nope"));
    }

    #[test]
    fn settling_twice_lands_in_the_same_place() {
        // `from_path` has already settled it, so a caller that settles again
        // against a different directory must not push it further. Idempotence is
        // what makes the public method safe to call without knowing.
        let (dir, mut cfg) = loaded(
            r#"
deny = ["scratch"]
[workspace]
name = "w"
"#,
        );
        let once = cfg.deny[0].path.clone();
        cfg.settle_deny(Path::new("/somewhere/else"));
        assert_eq!(cfg.deny[0].path, once);
        assert_eq!(once, dir.path().join("scratch"));
    }
}
