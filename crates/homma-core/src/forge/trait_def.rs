//! `Forge` trait + the value types it operates on.
//!
//! The trait is the abstract interface every concrete client (`ForgejoClient`
//! for Codeberg / Forgejo / Gitea, `GitHubClient` for github.com / Enterprise)
//! implements. The migrate command programs against the trait; the choice of
//! concrete client is driven by [`crate::config::ForgeKind`].

use super::error::ForgeError;

/// Operations every forge client supports.
///
/// Sync, not async. `homma` is a CLI that performs serial network steps
/// (create destination repo, push mirror, archive source); concurrency would
/// not change the wall-clock cost meaningfully, and the sync surface keeps
/// `homma-core` free of a tokio runtime dependency.
pub trait Forge {
    /// Read repo metadata from the forge.
    ///
    /// Returns [`ForgeError::RepoNotFound`] when the named repo does not
    /// exist. Used by the migrate command to copy description / default
    /// branch / topics / visibility from the source repo to the destination.
    fn fetch_repo(&self, owner: &str, name: &str) -> Result<RepoMetadata, ForgeError>;

    /// Check whether a repo exists without fetching its metadata.
    ///
    /// Cheaper than [`Self::fetch_repo`] when the caller only needs the
    /// existence answer (typical for migrate-idempotence checks).
    fn repo_exists(&self, owner: &str, name: &str) -> Result<bool, ForgeError>;

    /// Create a new repo in the named owner namespace.
    ///
    /// Returns the metadata of the newly created repo. Returns
    /// [`ForgeError::RepoAlreadyExists`] when a repo with the same name
    /// already lives under the owner.
    fn create_repo(
        &self,
        owner: &str,
        spec: &CreateRepoSpec,
    ) -> Result<RepoMetadata, ForgeError>;

    /// Mark a repo as archived (read-only). Used as the final step of a
    /// migration when the source is to be retained as a frozen artefact
    /// rather than deleted outright.
    fn archive_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError>;

    /// Delete a repo permanently. Used as the final step of a migration when
    /// the source is to be removed; the caller is expected to have already
    /// confirmed the destination push succeeded.
    fn delete_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError>;
}

/// Snapshot of a repo as the forge sees it.
///
/// The shared subset of `GET /repos/{owner}/{name}` across GitHub and
/// Forgejo. Concrete clients map their per-forge JSON shape into this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMetadata {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub visibility: Visibility,
    pub topics: Vec<String>,
    pub archived: bool,
    /// HTTPS clone URL as the forge reports it. The migrate command prefers
    /// this over [`super::url::clone_https`] when copying source metadata,
    /// because the forge may return a redirected canonical URL.
    pub clone_url_https: String,
}

/// Visibility of a repo on the forge.
///
/// GitHub uses `private: bool`; Forgejo uses `private: bool` plus an `internal`
/// state for org-internal visibility. We collapse to the three-way enum and
/// let concrete clients map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    /// Org-internal (visible to org members but not the public). Forgejo only;
    /// GitHub Enterprise has a similar concept but the open-source API does
    /// not expose it.
    Internal,
}

/// Inputs to [`Forge::create_repo`].
///
/// The shared subset of repo-creation parameters across GitHub and Forgejo.
/// Optional fields default to "forge picks" when `None`.
#[derive(Debug, Clone)]
pub struct CreateRepoSpec {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Visibility,
    /// Initial default branch. When `None`, the forge picks (usually `main`).
    pub default_branch: Option<String>,
    /// Skip the forge's auto-init (README / LICENSE / .gitignore generation).
    /// `true` for migrate destinations: the push lands the actual content,
    /// and an empty repo is required for an unrelated-history-free push.
    pub auto_init: bool,
}

impl CreateRepoSpec {
    /// New spec with sensible migrate-destination defaults: no auto-init,
    /// no description, public visibility, forge-default branch.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            visibility: Visibility::Public,
            default_branch: None,
            auto_init: false,
        }
    }

    /// Copy the migrate-relevant fields from a source repo's metadata onto
    /// this spec. Used by the migrate command to replicate description /
    /// visibility / default branch on the destination.
    pub fn replicate_from(mut self, source: &RepoMetadata) -> Self {
        self.description = source.description.clone();
        self.visibility = source.visibility;
        self.default_branch = Some(source.default_branch.clone());
        self
    }
}
