//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! gix-backed repository operations layer for homma.
//!
//! Public surface:
//! - [`RepoOps`] trait: operations every backend supports.
//! - [`GixRepo`]: the canonical `gix`-backed impl.
//! - [`Status`], [`Branch`], [`Remote`], [`TrackingStatus`], [`MirrorOpts`]: value types.
//! - [`RepoError`]: concrete error type returned by every op.
//! - [`hooks_path_at`], [`hooks_are_wired`]: whether per-repo git hooks fire at all.
//!
//! Design rationale lives in the `project-homma-repo-ops-design` memory note.

pub mod error;
pub mod git_impl;
pub mod gix_impl;
pub mod hooks;
pub mod ops;

pub use error::RepoError;
pub use git_impl::GixGit;
pub use gix_impl::GixRepo;
pub use hooks::{hooks_are_wired, hooks_path_at};
pub use ops::{Branch, MirrorOpts, Remote, RepoOps, Status, TrackingStatus, canonical_refspecs};
