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
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub forges: BTreeMap<String, ForgeConfig>,
    #[serde(default)]
    pub repos: BTreeMap<String, RepoConfig>,
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
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let s = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        Self::parse(&s)
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

/// `[defaults]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_license")]
    pub license: String,
    #[serde(default = "default_public_branch")]
    pub public_branch: String,
    #[serde(default = "default_working_branch")]
    pub working_branch: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            license: default_license(),
            public_branch: default_public_branch(),
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
    pub kind: ForgeKind,
    pub base_url: String,
    pub api_url: String,
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
    pub forge: String,
    pub owner: String,
    pub local_path: PathBuf,
    /// Overrides [`Defaults::public_branch`] when set.
    #[serde(default)]
    pub public_branch: Option<String>,
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
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "homma.toml parse error: {e}"),
            Self::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Io { source, .. } => Some(source),
        }
    }
}
