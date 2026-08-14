//! What standing a workspace up needs from git, as a contract rather than an
//! implementation.
//!
//! **Every path here is an [`AbsPath`]**, which is why no method checks whether
//! it was given a relative one. That precondition was prose, then a runtime
//! check in a single implementation, and both let three consecutive rounds ship
//! a route that walked out of it. In the signature it is checked once, by the
//! compiler.
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

use crate::path::AbsPath;

/// The git operations a workspace lifecycle performs.
pub trait Git {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether `path` already holds a repository.
    ///
    /// Standing up twice is the same answer, and this is what makes that true
    /// for the clone: a workspace that already has the content repository is
    /// left alone rather than re-cloned over.
    fn is_repo(&self, path: &AbsPath) -> bool;

    /// Clone `url` into `dest`, which does not yet exist as a repository.
    fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Self::Error>;

    /// Set the author identity in **`path`'s own configuration**.
    ///
    /// Never the global one. An identity written globally is invisible until a
    /// commit lands under whoever the machine belongs to, by which point the
    /// commits carrying the wrong author are already made.
    fn set_identity(&self, path: &AbsPath, name: &str, email: &str) -> Result<(), Self::Error>;

    /// Create a repository at `path`, which is not one yet.
    ///
    /// Used when the content repository is configured as `local`: the workspace
    /// is its own content repository, and on a fresh machine there is nothing
    /// there to clone from until one exists.
    fn init(&self, path: &AbsPath) -> Result<(), Self::Error>;

    /// The repository whose working tree `path` sits **inside**, if any, found
    /// by walking upward.
    ///
    /// **A path that is itself a repository is not inside one**, and reports
    /// `None`. The comparison lives here rather than in every caller because
    /// both sides have to be resolved to be compared at all: on a system where
    /// `/var` is a symlink to `/private/var`, a resolved ancestor and an
    /// unresolved subject are never equal, and a workspace refuses to stand up
    /// twice.
    ///
    /// `is_repo` only answers whether a path is itself a repository root, so a
    /// directory nested inside somebody else's checkout looks free. Initialising
    /// there produces a repository inside a repository and lands a participant's
    /// directories in a tree that is not ours.
    fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Self::Error>;

    /// The URL `path`'s `origin` remote points at, if it has one.
    ///
    /// **Not where the clone URL comes from.** That is configuration, and an
    /// earlier round derived it from here on the reasoning that a configuration
    /// key would duplicate the fact. The key already existed, the derivation
    /// consulted neither, and standing up from an unrelated clone cloned that
    /// unrelated repository. This exists to *cross-check* the configured URL
    /// against what a tree actually points at.
    fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Self::Error>;

    /// The author identity `path`'s own configuration carries, if any.
    ///
    /// Present so setting it can be asserted rather than assumed. A write with
    /// no read is a write nobody checks.
    fn identity(&self, path: &AbsPath) -> Result<Option<(String, String)>, Self::Error>;
}
