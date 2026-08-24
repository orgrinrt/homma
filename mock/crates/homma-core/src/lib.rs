//! `homma-core`: workspace management for multi-repo Rust workspaces.
//!
//! The crate is the engine layer beneath the `homma` CLI. It parses
//! `homma.toml` and exposes typed config and forge-profile shapes. The
//! `mockspace-config` and `mockspace-template` crates are re-exported for
//! consumers that want to invoke mockspace's template engine over their
//! own template context; homma no longer ships a built-in bridge from its
//! workspace config to mockspace's canonical config since the v2 schema
//! split the render-context concern from the config-schema concern (see
//! mockspace spec §47).

pub mod config;
pub mod forge;
pub mod repo;
pub mod testing;

pub use config::{
    Config,
    ConfigError,
    Defaults,
    ForgeConfig,
    ForgeKind,
    RepoConfig,
    WorkspaceConfig,
};
pub use forge::{
    CreateRepoSpec,
    Forge,
    ForgeError,
    ForgejoClient,
    GitHubClient,
    RepoMetadata,
    Visibility,
};
pub use mockspace_config as mockspace;
pub use mockspace_template as template;
pub use repo::{Branch, GixRepo, MirrorOpts, Remote, RepoError, RepoOps, Status, TrackingStatus};
