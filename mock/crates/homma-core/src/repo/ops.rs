//! `RepoOps` trait + its value types ([`Status`], [`Branch`], [`Remote`],
//! [`TrackingStatus`], [`MirrorOpts`]).
//!
//! The trait is the API homma's higher layers (CLI, migrate command,
//! forge clients) program against. [`super::GixRepo`] is the canonical
//! impl; alternative impls (test fixtures, fallback to the `git` binary)
//! can land later without API churn.

use super::error::RepoError;

/// Post-construction repo operations.
///
/// Constructors stay on the impl (`GixRepo::open`, `clone_into`, etc.)
/// because their shapes are backend-specific; the trait covers only
/// operations that make sense across every backend.
pub trait RepoOps {
    /// Full status: clean/dirty + current branch + tracking ahead/behind.
    fn status(&self) -> Result<Status, RepoError>;

    /// Current branch name, or `None` in detached-HEAD state.
    fn current_branch(&self) -> Result<Option<String>, RepoError>;

    /// All local branches.
    fn branches(&self) -> Result<Vec<Branch>, RepoError>;

    /// All configured remotes.
    fn remotes(&self) -> Result<Vec<Remote>, RepoError>;

    /// Add a new remote.
    fn add_remote(&mut self, name: &str, url: &str) -> Result<(), RepoError>;

    /// Remove a remote by name.
    fn remove_remote(&mut self, name: &str) -> Result<(), RepoError>;

    /// Switch the working tree to an existing local branch.
    fn checkout(&mut self, branch: &str) -> Result<(), RepoError>;

    /// Create a new local branch from `from` (branch name or ref). Does not switch to it.
    fn create_branch(&mut self, name: &str, from: &str) -> Result<(), RepoError>;
}

/// Working-tree + tracking snapshot.
#[derive(Debug, Clone)]
pub struct Status {
    pub current_branch: Option<String>,
    pub is_clean: bool,
    /// Count of worktree-vs-index changes (untracked files excluded). Does
    /// not include staged-but-uncommitted (index-vs-HEAD) changes; the
    /// `is_clean` flag covers both sides, this counter covers one. Naming
    /// reflects what is counted, not what callers may casually expect.
    pub worktree_changes: usize,
    pub tracking: Option<TrackingStatus>,
}

/// Local-vs-upstream divergence.
#[derive(Debug, Clone)]
pub struct TrackingStatus {
    pub remote_branch: String,
    pub ahead: usize,
    pub behind: usize,
}

/// A local branch entry.
#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub head_commit: String,
}

/// A configured remote.
#[derive(Debug, Clone)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

/// Options for [`super::GixRepo::mirror_into`].
///
/// The defaults preserve canonical-git refs and drop forge-specific
/// namespaces (`refs/pull/*`, `refs/merge-requests/*`, `refs/changes/*`).
/// See `project-homma-repo-ops-design` memory note for rationale.
#[derive(Debug, Clone)]
pub struct MirrorOpts {
    /// Refspecs to fetch and store under the local `refs/` tree.
    /// Each entry follows git refspec syntax (`+src:dst` or `src:dst`).
    pub include_refspecs: Vec<String>,
}

impl Default for MirrorOpts {
    fn default() -> Self {
        Self {
            include_refspecs: canonical_refspecs(),
        }
    }
}

/// The canonical-git fetch refspecs for a curated mirror.
///
/// Each entry is `+src:dst` so a re-run force-updates existing refs.
/// Forge-specific namespaces (`refs/pull/*`, `refs/merge-requests/*`,
/// `refs/changes/*`) are excluded by omission.
pub fn canonical_refspecs() -> Vec<String> {
    vec![
        "+refs/heads/*:refs/heads/*".into(),
        "+refs/tags/*:refs/tags/*".into(),
        "+refs/notes/*:refs/notes/*".into(),
        "+refs/replace/*:refs/replace/*".into(),
    ]
}
