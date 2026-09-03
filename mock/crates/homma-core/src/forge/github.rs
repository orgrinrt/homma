//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `GitHubClient`: a [`Forge`] impl for `github.com` (and API-compatible
//! Enterprise installations).
//!
//! ## Endpoint summary
//!
//! - `GET    /repos/{owner}/{name}`          read repo metadata
//! - `POST   /user/repos`                    create repo in token-user namespace
//! - `POST   /orgs/{owner}/repos`            create repo in org namespace
//! - `PATCH  /repos/{owner}/{name}`          archive (body `{"archived": true}`)
//! - `DELETE /repos/{owner}/{name}`          delete (needs `delete_repo` scope)
//!
//! ## Auth
//!
//! When `ForgeConfig::token_env` names an environment variable, the client
//! reads it at construction and sends `Authorization: Bearer <value>` on every
//! request. Bearer works with both classic personal access tokens and the
//! newer fine-grained PATs, so the client does not branch on token shape.
//! Missing token + auth-required endpoint surfaces as
//! [`ForgeError::Unauthorized`] from the forge.
//!
//! ## Required headers
//!
//! Every request carries `Accept: application/vnd.github+json` and
//! `X-GitHub-Api-Version: 2022-11-28`. GitHub uses the api-version header to
//! pin response shape across breaking changes; pinning here avoids silent
//! drift if GitHub releases a new default version. The User-Agent header is
//! also required by GitHub's API; the `ureq` agent is configured with one at
//! construction.
//!
//! ## Default branch
//!
//! GitHub's create-repo endpoint does NOT accept a `default_branch` field in
//! the request body; the default is taken from the org's `default_branch_name`
//! setting (typically `main`). [`Self::create_repo`] therefore ignores
//! `spec.default_branch`; the migrate command issues a follow-up
//! `PATCH /repos/{owner}/{name}` with `{"default_branch": "..."}` after the
//! mirror push lands the actual ref. The trait contract is preserved: the
//! returned metadata reflects GitHub's response, not the spec input.

use serde::{Deserialize, Serialize};

use super::url::api_root;
use super::{CommitStatus, CreateRepoSpec, Forge, ForgeError, OwnerKind, RepoMetadata, Visibility};
use crate::config::ForgeConfig;

const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";

/// A [`Forge`] backed by the GitHub REST v3 API.
pub struct GitHubClient {
    api_url: String,
    token:   Option<String>,
    agent:   ureq::Agent,
}

impl GitHubClient {
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
                // GitHub answers `301` for a repo that has been renamed, to
                // `/repositories/{id}` on the same host. ureq's default is to
                // drop `Authorization` on every redirect, so the followed
                // request arrives anonymous, a private repo answers `404`, and
                // `repo_exists` reports a repo that exists as absent. A rename
                // is the ordinary reason an owner or name goes stale, which is
                // the case the existence check is for.
                //
                // `SameHost` keeps the credential where the redirect stays on
                // the host it was issued for and drops it where it does not,
                // which is the property worth having rather than `Always`.
                .redirect_auth_headers(ureq::RedirectAuthHeaders::SameHost)
                .build(),
        }
    }

    /// Path under the API root for a per-repo resource.
    fn repo_path(&self, owner: &str, name: &str) -> String {
        format!("{}/repos/{owner}/{name}", self.api_url)
    }

    /// Path under the API root for the create-repo endpoint.
    ///
    /// `OwnerKind::User` always uses `/user/repos` (the token's user is
    /// implied; the explicit `owner` argument is informational and used only
    /// for error context). `OwnerKind::Org` uses `/orgs/{owner}/repos`.
    fn create_path(&self, owner: &str, kind: OwnerKind) -> String {
        match kind {
            OwnerKind::User => format!("{}/user/repos", self.api_url),
            OwnerKind::Org => format!("{}/orgs/{owner}/repos", self.api_url),
        }
    }

    fn auth_header(&self) -> Option<(&str, String)> {
        self.token
            .as_ref()
            .map(|t| ("Authorization", format!("Bearer {t}")))
    }

    fn apply_common_headers(&self, mut req: ureq::Request) -> ureq::Request {
        req = req
            .set("Accept", ACCEPT)
            .set("X-GitHub-Api-Version", API_VERSION);
        if let Some((k, v)) = self.auth_header() {
            req = req.set(k, &v);
        }
        req
    }

    fn get(&self, url: &str) -> Result<ureq::Response, ureq::Error> {
        let req = self.apply_common_headers(self.agent.get(url));
        req.call()
    }

    fn post_json<T: Serialize>(&self, url: &str, body: &T) -> Result<ureq::Response, ureq::Error> {
        let req = self.apply_common_headers(self.agent.post(url));
        req.send_json(body)
    }

    fn patch_json<T: Serialize>(&self, url: &str, body: &T) -> Result<ureq::Response, ureq::Error> {
        let req = self.apply_common_headers(self.agent.request("PATCH", url));
        req.send_json(body)
    }

    fn delete(&self, url: &str) -> Result<ureq::Response, ureq::Error> {
        let req = self.apply_common_headers(self.agent.delete(url));
        req.call()
    }
}

impl Forge for GitHubClient {
    fn fetch_repo(&self, owner: &str, name: &str) -> Result<RepoMetadata, ForgeError> {
        let url = self.repo_path(owner, name);
        match self.get(&url) {
            Ok(resp) => {
                let wire: GitHubRepo = resp.into_json().map_err(box_backend)?;
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

    /// Create a repo on GitHub.
    ///
    /// `spec.default_branch` is ignored: GitHub's create endpoint has no field
    /// for it. The migrate command sets the default branch via a follow-up
    /// `PATCH /repos/{owner}/{name}` after the mirror push lands the ref.
    ///
    /// `spec.visibility` of [`Visibility::Internal`] is collapsed to
    /// `private = true` on the wire, because the GitHub REST v3 create-repo
    /// body has no `visibility` enum (only `private: bool`). True org-internal
    /// visibility on Enterprise installations requires a follow-up PATCH that
    /// sets `visibility = "internal"`; github.com Free plans have no internal
    /// concept and the collapse is the correct mapping there.
    fn create_repo(&self, owner: &str, spec: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
        let url = self.create_path(owner, spec.owner_kind);
        let body = GitHubCreateBody::from_spec(spec);
        match self.post_json(&url, &body) {
            Ok(resp) => {
                let wire: GitHubRepo = resp.into_json().map_err(box_backend)?;
                Ok(wire.into_metadata())
            },
            // GitHub uses 422 Unprocessable Entity for "name already exists";
            // Forgejo uses 409. Both map to RepoAlreadyExists here.
            Err(ureq::Error::Status(422, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if body.contains("name already exists")
                    || body.to_ascii_lowercase().contains("already exists")
                {
                    Err(ForgeError::RepoAlreadyExists {
                        owner: owner.into(),
                        name:  spec.name.clone(),
                    })
                } else {
                    Err(ForgeError::UnexpectedStatus {
                        status: 422,
                        body:   truncate(body, 512),
                    })
                }
            },
            Err(e) => Err(map_ureq_error(e, owner, &spec.name)),
        }
    }

    fn archive_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError> {
        let url = self.repo_path(owner, name);
        let body = GitHubPatchBody {
            archived:       Some(true),
            default_branch: None,
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

    /// `POST {api}/repos/{owner}/{name}/statuses/{sha}`. GitHub creates a new
    /// status per call and shows the newest one per context, so posting
    /// `pending` and then `success` on the same context is the normal shape.
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

    /// `GET {api}/repos/{owner}/{name}/commits/{sha}`. GitHub answers a sha it
    /// has not received with 422; its 404 is the repository, absent or
    /// invisible to the token, which `map_ureq_error` reports as such rather
    /// than as a commit still on its way.
    fn commit_known(&self, owner: &str, name: &str, sha: &str) -> Result<bool, ForgeError> {
        let url = format!("{}/commits/{sha}", self.repo_path(owner, name));
        match self.get(&url) {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(422, _)) => Ok(false),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }

    /// `POST {api}/repos/{owner}/{name}/releases`, named after the tag.
    fn create_release(
        &self,
        owner: &str,
        name: &str,
        tag: &str,
        body: &str,
    ) -> Result<(), ForgeError> {
        let url = format!("{}/releases", self.repo_path(owner, name));
        let payload = super::trait_def::ReleaseBody {
            tag_name: tag.into(),
            name:     tag.into(),
            body:     body.into(),
        };
        match self.post_json(&url, &payload) {
            Ok(_) => Ok(()),
            Err(e) => Err(map_ureq_error(e, owner, name)),
        }
    }
}

/// Wire shape of `GET /repos/{owner}/{name}` (and create / patch responses).
/// Captures the migrate-relevant fields; ignores the rest.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GitHubRepo {
    pub name:           String,
    pub owner:          GitHubOwner,
    pub description:    Option<String>,
    pub default_branch: String,
    pub private:        bool,
    /// GitHub's newer visibility enum (`public` / `private` / `internal`).
    /// Returned alongside `private` on Enterprise installations. On
    /// `github.com` Free plans this is usually present but only carries
    /// `public` or `private`. Missing is tolerated for older API surfaces
    /// or proxied installations that strip it.
    #[serde(default)]
    pub visibility:     Option<String>,
    #[serde(default)]
    pub archived:       bool,
    pub clone_url:      String,
    #[serde(default)]
    pub topics:         Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GitHubOwner {
    pub login: String,
}

impl GitHubRepo {
    /// Map the wire shape to the trait's [`RepoMetadata`].
    ///
    /// Visibility resolution prefers the `visibility` string when present (it
    /// carries `internal` on Enterprise), falling back to the `private` bool.
    pub(crate) fn into_metadata(self) -> RepoMetadata {
        let visibility = match self.visibility.as_deref() {
            Some("internal") => Visibility::Internal,
            Some("private") => Visibility::Private,
            Some("public") => Visibility::Public,
            // Missing or unknown string: fall back to the bool.
            _ => {
                if self.private {
                    Visibility::Private
                } else {
                    Visibility::Public
                }
            },
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
///
/// GitHub accepts `name`, `description`, `private`, `auto_init`, and a long
/// tail of optional fields we do not need (`gitignore_template`,
/// `license_template`, `homepage`, `has_issues`, ...). The body intentionally
/// excludes `default_branch` because GitHub does not honor it at create time;
/// see [`GitHubClient::create_repo`] for the follow-up PATCH path.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitHubCreateBody {
    pub name:        String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub private:     bool,
    pub auto_init:   bool,
}

impl GitHubCreateBody {
    pub(crate) fn from_spec(spec: &CreateRepoSpec) -> Self {
        Self {
            name:        spec.name.clone(),
            description: spec.description.clone(),
            private:     matches!(spec.visibility, Visibility::Private | Visibility::Internal),
            auto_init:   spec.auto_init,
        }
    }
}

/// Body for the repo-update PATCH.
///
/// Carries the two fields the migrate command needs after a push: archived
/// (for end-of-migration teardown) and default_branch (for the post-push
/// branch rename). All optional; absent fields are not sent.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GitHubPatchBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived:       Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

/// Translate a `ureq::Error` into a [`ForgeError`].
///
/// Status-coded errors map to the trait's typed variants where the meaning is
/// shared across forges (404 -> RepoNotFound, 401/403 -> Unauthorized, other
/// codes -> UnexpectedStatus with body excerpt). Transport-level failures box
/// into [`ForgeError::Backend`].
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
///
/// GitHub uses 422 for "name already exists" (rather than 409 like Forgejo);
/// that mapping lives in [`GitHubClient::create_repo`] because the body needs
/// inspection to distinguish "duplicate name" from other 422 causes. Here we
/// treat unmatched 422s as `UnexpectedStatus`.
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
/// real-world API error bodies carry non-ASCII content (commit messages,
/// descriptions, validation hints). A naive `s.truncate(512)` against a
/// payload with multi-byte chars near byte 512 crashes the host process; the
/// boundary walk avoids that.
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
#[path = "github_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "github_wire_tests.rs"]
mod wire_tests;
