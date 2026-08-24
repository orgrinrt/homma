//! `RepoError`: error type returned by every [`super::RepoOps`] method
//! and by [`super::GixRepo`] constructors.
//!
//! Concrete (not associated). Alternative impls wrap their backend errors
//! as `RepoError::Backend(Box<dyn Error>)`. Lift to an associated type if
//! a real second impl forces the issue.

use std::path::PathBuf;

/// Errors from the gix wrapper layer.
#[derive(Debug)]
pub enum RepoError {
    /// `gix::open` failed (path is not a repo, permissions, etc.).
    Open(Box<gix::open::Error>),
    /// Network or pack-fetch failure during clone.
    Clone(Box<gix::clone::Error>),
    /// Fetch-pack failure during clone (after `prepare_clone`).
    Fetch(Box<gix::clone::fetch::Error>),
    /// Worktree checkout failure after fetch.
    Checkout(Box<gix::clone::checkout::main_worktree::Error>),
    /// Reference enumeration or peeling failed.
    References(String),
    /// Remote enumeration or manipulation failed.
    Remote(String),
    /// Status computation (clean/dirty, ahead/behind) failed.
    Status(String),
    /// Refspec parse / validation failure.
    Refspec(String),
    /// The requested branch was not found locally.
    BranchNotFound(String),
    /// Generic IO error tied to a path.
    Io {
        path:   PathBuf,
        source: std::io::Error,
    },
    /// Reading or writing the repository's own configuration failed.
    Config(String),
    /// Alternative-impl backend error pass-through.
    Backend(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(e) => write!(f, "failed to open repository: {e}"),
            Self::Clone(e) => write!(f, "clone failed: {e}"),
            Self::Fetch(e) => write!(f, "fetch failed: {e}"),
            Self::Checkout(e) => write!(f, "worktree checkout failed: {e}"),
            Self::References(msg) => write!(f, "reference error: {msg}"),
            Self::Remote(msg) => write!(f, "remote error: {msg}"),
            Self::Status(msg) => write!(f, "status error: {msg}"),
            Self::Refspec(msg) => write!(f, "refspec error: {msg}"),
            Self::BranchNotFound(name) => write!(f, "branch not found: {name}"),
            Self::Io {
                path,
                source,
            } => write!(f, "io error at {}: {source}", path.display()),
            Self::Config(msg) => write!(f, "config error: {msg}"),
            Self::Backend(e) => write!(f, "backend error: {e}"),
        }
    }
}

impl std::error::Error for RepoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open(e) => Some(e.as_ref()),
            Self::Clone(e) => Some(e.as_ref()),
            Self::Fetch(e) => Some(e.as_ref()),
            Self::Checkout(e) => Some(e.as_ref()),
            Self::Io {
                source,
                ..
            } => Some(source),
            Self::Backend(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
