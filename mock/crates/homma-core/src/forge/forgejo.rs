//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `ForgejoClient`: a [`Forge`] impl for Codeberg, Forgejo, and Gitea.
//!
//! The three target hosts share the same REST API shape (Forgejo forked from
//! Gitea; Codeberg runs Forgejo). One client serves all three.
//!
//! ## Endpoint summary
//!
//! - `GET    /repos/{owner}/{name}`          read repo metadata
//! - `POST   /user/repos`                    create repo in token-user namespace
//! - `POST   /orgs/{owner}/repos`            create repo in org namespace
//! - `PATCH  /repos/{owner}/{name}`          archive (body `{"archived": true}`)
//! - `DELETE /repos/{owner}/{name}`          delete
//!
//! ## Auth
//!
//! When `ForgeConfig::token_env` names an environment variable, the client
//! reads it at construction and sends `Authorization: token <value>` on every
//! request. Missing token + auth-required endpoint surfaces as
//! [`ForgeError::Unauthorized`] from the forge.

use serde::{Deserialize, Serialize};

use super::url::api_root;
use super::{CommitStatus, CreateRepoSpec, Forge, ForgeError, OwnerKind, RepoMetadata, Visibility};
use crate::config::ForgeConfig;

/// A [`Forge`] backed by the Forgejo / Gitea REST API.
pub struct ForgejoClient {
    api_url: String,
    token:   Option<String>,
    agent:   ureq::Agent,
}

impl ForgejoClient {
    /// Construct from a [`ForgeConfig`].
    ///
    /// Takes the token from the environment variable named by
    /// `forge.token_env`, or from `forge.token_cmd` when that names none or
    /// holds nothing. See [`crate::forge::token`].
    ///
    /// Neither producing one gives a tokenless client, which is anonymous
    /// access rather than an error: read-only operations still work for public
    /// repos, and mutating ones return [`ForgeError::Unauthorized`].
    pub fn new(forge: &ForgeConfig) -> Self {
        Self::with_token_opt(api_root(forge), crate::forge::token::resolve(forge))
    }

    /// Construct with an explicit token override.
    pub fn with_token(api_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self::with_token_opt(api_url.into(), Some(token.into()))
    }

    /// Construct with no token (anonymous access).
    pub fn anonymous(api_url: impl Into<String>) -> Self {
        Self::with_token_opt(api_url.into(), None)
    }

    fn with_token_opt(api_url: String, token: Option<String>) -> Self {
        Self {
            api_url,
            token,
            agent: ureq::AgentBuilder::new()
                .user_agent(concat!("homma/", env!("CARGO_PKG_VERSION")))
                .build(),
        }
    }

    /// Path under the API root for a per-repo resource.
    fn repo_path(&self, owner: &str, name: &str) -> String {
        format!("{}/repos/{owner}/{name}", self.api_url)
    }

    /// Path under the API root for the create-repo endpoint, dispatched by
    /// [`OwnerKind`] per the [`CreateRepoSpec`].
    fn create_path(&self, owner: &str, kind: OwnerKind) -> String {
        match kind {
            OwnerKind::User => format!("{}/user/repos", self.api_url),
            OwnerKind::Org => format!("{}/orgs/{owner}/repos", self.api_url),
        }
    }

    fn auth_header(&self) -> Option<(&str, String)> {
        self.token
            .as_ref()
            .map(|t| ("Authorization", format!("token {t}")))
    }

    fn get(&self, url: &str) -> Result<ureq::Response, ureq::Error> {
        let mut req = self.agent.get(url);
        if let Some((k, v)) = self.auth_header() {
            req = req.set(k, &v);
        }
        req.call()
    }

    fn post_json<T: Serialize>(&self, url: &str, body: &T) -> Result<ureq::Response, ureq::Error> {
        let mut req = self.agent.post(url);
        if let Some((k, v)) = self.auth_header() {
            req = req.set(k, &v);
        }
        req.send_json(body)
    }

    fn patch_json<T: Serialize>(&self, url: &str, body: &T) -> Result<ureq::Response, ureq::Error> {
        let mut req = self.agent.request("PATCH", url);
        if let Some((k, v)) = self.auth_header() {
            req = req.set(k, &v);
        }
        req.send_json(body)
    }

    fn delete(&self, url: &str) -> Result<ureq::Response, ureq::Error> {
        let mut req = self.agent.delete(url);
        if let Some((k, v)) = self.auth_header() {
            req = req.set(k, &v);
        }
        req.call()
    }
}

impl Forge for ForgejoClient {
    fn fetch_repo(&self, owner: &str, name: &str) -> Result<RepoMetadata, ForgeError> {
        let url = self.repo_path(owner, name);
        match self.get(&url) {
            Ok(resp) => {
                let wire: ForgejoRepo = resp.into_json().map_err(box_backend)?;
                Ok(wire.into_metadata())
            },
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }

    fn repo_exists(&self, owner: &str, name: &str) -> Result<bool, ForgeError> {
        let url = self.repo_path(owner, name);
        match self.get(&url) {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(404, _)) => Ok(false),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }

    /// Create a repo on the forge.
    ///
    /// `spec.visibility` of [`Visibility::Internal`] is collapsed to
    /// `private = true` on the wire, because Forgejo's create endpoint has
    /// no separate `internal` field. Callers needing true org-internal
    /// visibility must follow up with a `PATCH /repos/{owner}/{name}` that
    /// sets `internal = true` once the repo exists. [`crate::Forge`]'s migrate
    /// path handles this when source-side visibility is Internal.
    fn create_repo(&self, owner: &str, spec: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
        let url = self.create_path(owner, spec.owner_kind);
        let body = ForgejoCreateBody::from_spec(spec);
        match self.post_json(&url, &body) {
            Ok(resp) => {
                let wire: ForgejoRepo = resp.into_json().map_err(box_backend)?;
                Ok(wire.into_metadata())
            },
            Err(ureq::Error::Status(409, _)) => {
                Err(ForgeError::RepoAlreadyExists {
                    owner: owner.into(),
                    name:  spec.name.clone(),
                })
            },
            Err(e) => Err(map_ureq_error(e, owner, &spec.name)),
        }
    }

    fn archive_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError> {
        let url = self.repo_path(owner, name);
        let body = ForgejoPatchBody {
            archived: Some(true),
        };
        match self.patch_json(&url, &body) {
            Ok(_) => Ok(()),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }

    fn delete_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError> {
        let url = self.repo_path(owner, name);
        match self.delete(&url) {
            Ok(_) => Ok(()),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }

    /// `GET {api}/user`: the endpoint both forges answer only for an accepted
    /// credential. `401` is the rejection; `403` counts as accepted, because it
    /// says the credential was recognised and the account is not permitted,
    /// which is a different problem and not one this question asks about.
    fn credential_works(&self) -> Result<bool, ForgeError> {
        let url = format!("{}/user", self.api_url);
        match self.get(&url) {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(401, _)) => Ok(false),
            Err(ureq::Error::Status(403, _)) => Ok(true),
            Err(e) => Err(map_ureq_error(e, "", "user")),
        }
    }

    /// `POST {api}/repos/{owner}/{name}/statuses/{sha}`, the same body shape
    /// GitHub takes; Forgejo also accepts `warning`, which nothing here sends.
    fn set_commit_status(
        &self,
        owner: &str,
        name: &str,
        sha: &str,
        status: &CommitStatus,
    ) -> Result<(), ForgeError> {
        let url = format!("{}/statuses/{sha}", self.repo_path(owner, name));
        match self.post_json(&url, status) {
            Ok(_) => Ok(()),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }
}

/// Wire shape of `GET /repos/{owner}/{name}` (and the response of create-repo
/// and patch). Captures the migrate-relevant fields; ignores the rest.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ForgejoRepo {
    pub name:           String,
    pub owner:          ForgejoOwner,
    pub description:    Option<String>,
    pub default_branch: String,
    pub private:        bool,
    #[serde(default)]
    pub internal:       bool,
    #[serde(default)]
    pub archived:       bool,
    pub clone_url:      String,
    #[serde(default)]
    pub topics:         Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ForgejoOwner {
    pub login: String,
}

impl ForgejoRepo {
    /// Map the wire shape to the trait's [`RepoMetadata`].
    pub(crate) fn into_metadata(self) -> RepoMetadata {
        let visibility = if self.private {
            Visibility::Private
        } else if self.internal {
            Visibility::Internal
        } else {
            Visibility::Public
        };
        RepoMetadata {
            owner: self.owner.login,
            name: self.name,
            description: self.description.filter(|s| !s.is_empty()),
            default_branch: self.default_branch,
            visibility,
            topics: self.topics,
            archived: self.archived,
            clone_url_https: self.clone_url,
        }
    }
}

/// Body for the create-repo endpoint.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ForgejoCreateBody {
    pub name:           String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:    Option<String>,
    pub private:        bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub auto_init:      bool,
}

impl ForgejoCreateBody {
    pub(crate) fn from_spec(spec: &CreateRepoSpec) -> Self {
        Self {
            name:           spec.name.clone(),
            description:    spec.description.clone(),
            private:        matches!(spec.visibility, Visibility::Private | Visibility::Internal),
            default_branch: spec.default_branch.clone(),
            auto_init:      spec.auto_init,
        }
    }
}

/// Body for the archive patch.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ForgejoPatchBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

/// Translate a `ureq::Error` into a [`ForgeError`].
///
/// Status-coded errors map to the trait's typed variants where the meaning is
/// shared across forges (404 -> RepoNotFound, 401/403 -> Unauthorized,
/// other codes -> UnexpectedStatus with body excerpt). Transport-level
/// failures box into [`ForgeError::Backend`].
pub(crate) fn map_ureq_error(e: ureq::Error, owner: &str, name: &str) -> ForgeError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            map_status(code, owner, name, body)
        },
        ureq::Error::Transport(t) => ForgeError::Backend(Box::new(t)),
    }
}

/// Pure mapping from `(status, body)` to a `ForgeError`. Lifted out so tests
/// can exercise the error matrix without spinning up a real client.
pub(crate) fn map_status(status: u16, owner: &str, name: &str, body: String) -> ForgeError {
    match status {
        404 => {
            ForgeError::RepoNotFound {
                owner: owner.into(),
                name:  name.into(),
            }
        },
        401 | 403 => {
            ForgeError::Unauthorized {
                reason: body,
            }
        },
        409 => {
            ForgeError::RepoAlreadyExists {
                owner: owner.into(),
                name:  name.into(),
            }
        },
        _ => {
            ForgeError::UnexpectedStatus {
                status,
                body: truncate(body, 512),
            }
        },
    }
}

fn box_backend<E: std::error::Error + Send + Sync + 'static>(e: E) -> ForgeError {
    ForgeError::Backend(Box::new(e))
}

/// Truncate `s` to at most `max` bytes plus a trailing marker, walking back
/// to the nearest UTF-8 char boundary so the result is always valid UTF-8.
///
/// `String::truncate` panics if `max` does not fall on a char boundary, and
/// real-world forge responses carry non-ASCII content (commit messages,
/// descriptions). A naive `s.truncate(512)` against `"ä".repeat(N)` crashes
/// the host process; the boundary walk avoids that.
fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push_str("... [truncated]");
    }
    s
}

#[cfg(test)]
#[path = "forgejo_tests.rs"]
mod tests;
