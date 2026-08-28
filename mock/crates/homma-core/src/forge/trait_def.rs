//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

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
    /// A lighter-weight existence probe than [`Self::fetch_repo`] when the
    /// caller only needs a yes/no answer (typical for migrate-idempotence
    /// checks). Concrete clients may end up calling the same REST endpoint
    /// as `fetch_repo` and discarding the body; the trait method still
    /// reads cleaner at call sites that don't want the metadata.
    fn repo_exists(&self, owner: &str, name: &str) -> Result<bool, ForgeError>;

    /// Create a new repo in the named owner namespace.
    ///
    /// Returns the metadata of the newly created repo. Returns
    /// [`ForgeError::RepoAlreadyExists`] when a repo with the same name
    /// already lives under the owner.
    fn create_repo(&self, owner: &str, spec: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError>;

    /// Mark a repo as archived (read-only). Used as the final step of a
    /// migration when the source is to be retained as a frozen artefact
    /// rather than deleted outright.
    fn archive_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError>;

    /// Delete a repo permanently. Used as the final step of a migration when
    /// the source is to be removed; the caller is expected to have already
    /// confirmed the destination push succeeded.
    fn delete_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError>;

    /// Whether the credential this client carries is accepted by the forge.
    ///
    /// A negative answer from [`Self::repo_exists`] is evidence only when the
    /// asker could have seen a positive one. Both forges answer `404` for a
    /// private repo the credential cannot see, identically to one that is not
    /// there, so a missing, expired or revoked token turns every private repo
    /// into a false absence. Checking that a token is *set* covers none of
    /// those, and this does.
    ///
    /// **Scope is not covered, and the shipped clients cannot cover it.** Both
    /// ask `GET /user`, which answers whether the forge knows the credential,
    /// not what the credential is allowed to read. A token that authenticates
    /// and lacks repository read still turns a private repo into a reported
    /// absence, and this returns `Ok(true)` for it. Covering that means probing
    /// a repository the caller already expects to be there, which is a
    /// different question with a different signature.
    ///
    /// `Ok(false)` is the forge rejecting the credential. Anything that leaves
    /// the question unanswered, a network failure or an unexpected status, is
    /// an error, because it is not evidence either way.
    fn credential_works(&self) -> Result<bool, ForgeError>;
}

/// Snapshot of a repo as the forge sees it.
///
/// The shared subset of `GET /repos/{owner}/{name}` across GitHub and
/// Forgejo. Concrete clients map their per-forge JSON shape into this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMetadata {
    pub owner:           String,
    pub name:            String,
    pub description:     Option<String>,
    pub default_branch:  String,
    pub visibility:      Visibility,
    pub topics:          Vec<String>,
    pub archived:        bool,
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
    pub name:           String,
    pub description:    Option<String>,
    pub visibility:     Visibility,
    /// Whether the `owner` namespace is a user account or an organisation.
    ///
    /// GitHub's create endpoint (`POST /user/repos`) ignores this because the
    /// token's user is implied; the field is still required for the Forgejo
    /// client, which dispatches between `POST /orgs/{owner}/repos` (when
    /// `Org`) and `POST /user/repos` (when `User`). `homma.toml` carries the
    /// answer per repo, so the caller passes it through without needing an
    /// extra probe.
    pub owner_kind:     OwnerKind,
    /// Initial default branch. When `None`, the forge picks (usually `main`).
    pub default_branch: Option<String>,
    /// Skip the forge's auto-init (README / LICENSE / .gitignore generation).
    /// `true` for migrate destinations: the push lands the actual content,
    /// and an empty repo is required for an unrelated-history-free push.
    pub auto_init:      bool,
}

/// Whether an `owner` namespace is a user account or an organisation.
///
/// Used by [`Forge::create_repo`] to dispatch between user-scope and
/// org-scope create endpoints. GitHub ignores this (the token's user is
/// always implied); Forgejo / Gitea require the right scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerKind {
    User,
    Org,
}

impl CreateRepoSpec {
    /// New spec with sensible migrate-destination defaults: no auto-init,
    /// no description, public visibility, user-owned, forge-default branch.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name:           name.into(),
            description:    None,
            visibility:     Visibility::Public,
            owner_kind:     OwnerKind::User,
            default_branch: None,
            auto_init:      false,
        }
    }

    /// Mark the spec as targeting an organisation namespace. Builder helper
    /// for the common case where the caller knows the owner kind at spec-
    /// construction time.
    pub fn in_org(mut self) -> Self {
        self.owner_kind = OwnerKind::Org;
        self
    }

    /// Copy the migrate-relevant fields from a source repo's metadata onto
    /// this spec. Used by the migrate command to replicate description /
    /// visibility / default branch on the destination.
    ///
    /// Does not copy `topics`. Topics are typically set via a separate forge
    /// endpoint (`PUT /repos/{owner}/{name}/topics`), which the migrate command
    /// performs after the repo exists.
    pub fn replicate_from(mut self, source: &RepoMetadata) -> Self {
        self.description = source.description.clone();
        self.visibility = source.visibility;
        self.default_branch = Some(source.default_branch.clone());
        self
    }
}
