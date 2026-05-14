//! gix-orthogonal forge client layer for homma.
//!
//! `gix` covers the git protocol side of a migration (clone, mirror, push,
//! ref bookkeeping). The forge layer covers everything outside git: creating
//! the destination repo via the forge's REST API, replicating description /
//! visibility / default branch, and archiving or deleting the source. The
//! two layers compose: the migrate command reads source metadata via the
//! [`Forge`] trait, creates the destination via the same trait, mirrors via
//! [`crate::GixRepo::mirror_into`], and tears down the source via [`Forge`]
//! again.
//!
//! Public surface:
//! - [`Forge`] trait: operations every concrete client (`ForgejoClient`,
//!   `GitHubClient`) implements.
//! - [`RepoMetadata`], [`CreateRepoSpec`], [`Visibility`]: the value types
//!   the trait operates on.
//! - [`ForgeError`]: concrete error type.
//! - [`url`] module: pure URL composers (clone, web, api). Used by the
//!   migrate command to build URLs without instantiating a client.

pub mod error;
pub mod trait_def;
pub mod url;

pub use error::ForgeError;
pub use trait_def::{CreateRepoSpec, Forge, RepoMetadata, Visibility};
