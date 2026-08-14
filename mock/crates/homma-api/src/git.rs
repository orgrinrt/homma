//! What standing a workspace up needs from git, as a contract rather than an
//! implementation.
//!
//! Declared here because it is vocabulary: the registry crate has to say "clone
//! this and set that identity" without knowing which git library says it. No I/O
//! happens in this module, which is the rule for this crate; a trait describing
//! I/O is not performing any.
//!
//! The reason this is a trait at all, rather than a direct call into the git
//! crate, is that the lifecycle and the git operations fail differently and want
//! testing differently. The lifecycle wants a fake, so its own logic can be
//! checked without a repository. The implementation wants a real repository,
//! because a fake proves the wiring and says nothing about whether the identity
//! actually landed.

use std::path::Path;

/// The git operations a workspace lifecycle performs.
pub trait Git {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether `path` already holds a repository.
    ///
    /// Standing up twice is the same answer, and this is what makes that true
    /// for the clone: a workspace that already has the content repository is
    /// left alone rather than re-cloned over.
    fn is_repo(&self, path: &Path) -> bool;

    /// Clone `url` into `dest`, which does not yet exist as a repository.
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), Self::Error>;

    /// Set the author identity in **`path`'s own configuration**.
    ///
    /// Never the global one. An identity written globally is invisible until a
    /// commit lands under whoever the machine belongs to, by which point the
    /// commits carrying the wrong author are already made.
    fn set_identity(&self, path: &Path, name: &str, email: &str) -> Result<(), Self::Error>;

    /// The URL `path`'s `origin` remote points at, if it has one.
    ///
    /// This is where a content repository's clone URL comes from. homma runs
    /// inside a clone of it, so the URL is already on disk and correct; a
    /// configuration key for it would be a second place for one fact to live
    /// and therefore a second place for it to be wrong.
    fn origin_url(&self, path: &Path) -> Result<Option<String>, Self::Error>;

    /// The author identity `path`'s own configuration carries, if any.
    ///
    /// Present so setting it can be asserted rather than assumed. A write with
    /// no read is a write nobody checks.
    fn identity(&self, path: &Path) -> Result<Option<(String, String)>, Self::Error>;
}
