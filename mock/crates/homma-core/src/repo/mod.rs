//! gix-backed repository operations layer for homma.
//!
//! Public surface:
//! - [`RepoOps`] trait: operations every backend supports.
//! - [`GixRepo`]: the canonical `gix`-backed impl.
//! - [`Status`], [`Branch`], [`Remote`], [`TrackingStatus`], [`MirrorOpts`]: value types.
//! - [`RepoError`]: concrete error type returned by every op.
//!
//! Design rationale lives in the `project-homma-repo-ops-design` memory note.

pub mod error;
pub mod git_impl;
pub mod gix_impl;
pub mod ops;

pub use error::RepoError;
pub use git_impl::GixGit;
pub use gix_impl::GixRepo;
pub use ops::{canonical_refspecs, Branch, MirrorOpts, Remote, RepoOps, Status, TrackingStatus};
