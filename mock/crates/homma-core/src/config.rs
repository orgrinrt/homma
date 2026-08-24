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
        if cfg.workspace.path.is_relative() {
            let beside = path.parent().unwrap_or(Path::new("."));
            cfg.workspace.path = normalise(&beside.join(&cfg.workspace.path));
        }
        Ok(cfg)
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
