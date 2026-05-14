//! `homma-core`: workspace management for multi-repo Rust workspaces.
//!
//! The crate is the engine layer beneath the `homma` CLI. It parses
//! `homma.toml`, exposes typed config and forge-profile shapes, and re-exports
//! the mockspace template engine and canonical config schema so consumers can
//! render templates over homma's workspace context.

pub mod config;
pub mod forge;
pub mod mapping;
pub mod repo;

pub use config::{Config, ConfigError, Defaults, ForgeConfig, ForgeKind, RepoConfig, WorkspaceConfig};
pub use forge::{CreateRepoSpec, Forge, ForgeError, ForgejoClient, RepoMetadata, Visibility};
pub use repo::{
    Branch, GixRepo, MirrorOpts, Remote, RepoError, RepoOps, Status, TrackingStatus,
};

pub use mockspace_config as mockspace;
pub use mockspace_template as template;
