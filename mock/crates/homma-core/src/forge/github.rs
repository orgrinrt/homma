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
use super::{CreateRepoSpec, Forge, ForgeError, OwnerKind, RepoMetadata, Visibility};
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
    /// Reads the token from the env var named by `forge.token_env` when set.
    /// A missing env var produces a tokenless client (read-only ops still work
    /// for public repos; mutating ops return [`ForgeError::Unauthorized`]).
    pub fn new(forge: &ForgeConfig) -> Self {
        let token = forge
            .token_env
            .as_deref()
            .and_then(|name| std::env::var(name).ok());
        Self::with_token_opt(api_root(forge), token)
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
mod tests {
    use super::*;
    use crate::config::ForgeKind;

    fn cfg() -> ForgeConfig {
        ForgeConfig {
            kind:      ForgeKind::Github,
            base_url:  "https://github.com".into(),
            api_url:   "https://api.github.com".into(),
            token_env: None,
        }
    }

    #[test]
    fn create_path_user_owner() {
        let c = GitHubClient::anonymous("https://api.github.com");
        assert_eq!(
            c.create_path("orgrinrt", OwnerKind::User),
            "https://api.github.com/user/repos"
        );
    }

    #[test]
    fn create_path_org_owner() {
        let c = GitHubClient::anonymous("https://api.github.com");
        assert_eq!(
            c.create_path("hiisi-digital", OwnerKind::Org),
            "https://api.github.com/orgs/hiisi-digital/repos"
        );
    }

    #[test]
    fn repo_path_composes_api_url() {
        let c = GitHubClient::anonymous("https://api.github.com");
        assert_eq!(
            c.repo_path("orgrinrt", "homma"),
            "https://api.github.com/repos/orgrinrt/homma"
        );
    }

    #[test]
    fn new_reads_token_from_env() {
        let var = "HOMMA_TEST_GITHUB_TOKEN";
        std::env::set_var(var, "ghp_secret");
        let mut f = cfg();
        f.token_env = Some(var.into());
        let c = GitHubClient::new(&f);
        assert_eq!(c.token.as_deref(), Some("ghp_secret"));
        std::env::remove_var(var);
    }

    #[test]
    fn new_handles_missing_token_env() {
        let mut f = cfg();
        f.token_env = Some("HOMMA_TEST_DEFINITELY_UNSET_GH_VAR".into());
        let c = GitHubClient::new(&f);
        assert!(c.token.is_none());
    }

    #[test]
    fn auth_header_present_when_token_set() {
        let c = GitHubClient::with_token("https://api.github.com", "abc");
        let (k, v) = c.auth_header().unwrap();
        assert_eq!(k, "Authorization");
        assert_eq!(v, "Bearer abc");
    }

    #[test]
    fn auth_header_absent_when_anonymous() {
        let c = GitHubClient::anonymous("https://api.github.com");
        assert!(c.auth_header().is_none());
    }

    #[test]
    fn map_status_404_is_repo_not_found() {
        let e = map_status(404, "orgrinrt", "homma", "Not Found".into());
        assert!(matches!(e, ForgeError::RepoNotFound { .. }));
    }

    #[test]
    fn map_status_401_403_are_unauthorized() {
        assert!(matches!(
            map_status(401, "o", "n", "Bad credentials".into()),
            ForgeError::Unauthorized { .. }
        ));
        assert!(matches!(
            map_status(403, "o", "n", "rate limit exceeded".into()),
            ForgeError::Unauthorized { .. }
        ));
    }

    #[test]
    fn map_status_other_carries_status_and_body() {
        let e = map_status(502, "o", "n", "bad gateway".into());
        match e {
            ForgeError::UnexpectedStatus {
                status,
                body,
            } => {
                assert_eq!(status, 502);
                assert!(body.contains("bad gateway"));
            },
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn map_status_422_is_not_routed_to_already_exists_here() {
        // 422 routing lives in create_repo, not map_status, because the body
        // needs inspection to disambiguate "duplicate name" from other 422
        // causes (invalid name, missing required field, etc.).
        let e = map_status(422, "o", "n", "Validation Failed".into());
        assert!(matches!(e, ForgeError::UnexpectedStatus {
            status: 422,
            ..
        }));
    }

    #[test]
    fn map_status_truncates_long_body() {
        let big = "x".repeat(2048);
        let e = map_status(500, "o", "n", big);
        match e {
            ForgeError::UnexpectedStatus {
                body,
                ..
            } => {
                assert!(body.len() <= 512 + "... [truncated]".len());
                assert!(body.ends_with("... [truncated]"));
            },
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn map_status_truncates_multibyte_safely() {
        // Regression: String::truncate panics on non-char-boundary cuts.
        let big = "ä".repeat(300);
        let e = map_status(500, "o", "n", big);
        match e {
            ForgeError::UnexpectedStatus {
                body,
                ..
            } => {
                assert!(body.ends_with("... [truncated]"));
                assert!(std::str::from_utf8(body.as_bytes()).is_ok());
            },
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn github_repo_into_metadata_public() {
        let wire = GitHubRepo {
            name:           "homma".into(),
            owner:          GitHubOwner {
                login: "orgrinrt".into(),
            },
            description:    Some("workspace tooling".into()),
            default_branch: "dev".into(),
            private:        false,
            visibility:     Some("public".into()),
            archived:       false,
            clone_url:      "https://github.com/orgrinrt/homma.git".into(),
            topics:         vec!["rust".into()],
        };
        let m = wire.into_metadata();
        assert_eq!(m.owner, "orgrinrt");
        assert_eq!(m.name, "homma");
        assert_eq!(m.default_branch, "dev");
        assert_eq!(m.visibility, Visibility::Public);
        assert_eq!(m.topics, vec!["rust".to_string()]);
        assert!(!m.archived);
    }

    #[test]
    fn github_repo_into_metadata_private_via_string() {
        let wire = GitHubRepo {
            name:           "x".into(),
            owner:          GitHubOwner {
                login: "o".into(),
            },
            description:    None,
            default_branch: "main".into(),
            private:        true,
            visibility:     Some("private".into()),
            archived:       false,
            clone_url:      String::new(),
            topics:         Vec::new(),
        };
        assert_eq!(wire.into_metadata().visibility, Visibility::Private);
    }

    #[test]
    fn github_repo_into_metadata_internal_via_string() {
        // Enterprise-only on github.com REST v3, but the wire shape supports
        // it. Mapping must surface Internal so migrate can act on it.
        let wire = GitHubRepo {
            name:           "x".into(),
            owner:          GitHubOwner {
                login: "o".into(),
            },
            description:    None,
            default_branch: "main".into(),
            private:        false,
            visibility:     Some("internal".into()),
            archived:       false,
            clone_url:      String::new(),
            topics:         Vec::new(),
        };
        assert_eq!(wire.into_metadata().visibility, Visibility::Internal);
    }

    #[test]
    fn github_repo_into_metadata_falls_back_to_private_bool() {
        // No visibility string in response (older API surface or proxy):
        // fall back to the private bool.
        let wire = GitHubRepo {
            name:           "x".into(),
            owner:          GitHubOwner {
                login: "o".into(),
            },
            description:    None,
            default_branch: "main".into(),
            private:        true,
            visibility:     None,
            archived:       false,
            clone_url:      String::new(),
            topics:         Vec::new(),
        };
        assert_eq!(wire.into_metadata().visibility, Visibility::Private);
    }

    #[test]
    fn github_repo_into_metadata_unknown_string_falls_back_to_private_bool() {
        let wire = GitHubRepo {
            name:           "x".into(),
            owner:          GitHubOwner {
                login: "o".into(),
            },
            description:    None,
            default_branch: "main".into(),
            private:        false,
            visibility:     Some("weird".into()),
            archived:       false,
            clone_url:      String::new(),
            topics:         Vec::new(),
        };
        // Unknown string with private=false falls back to Public.
        assert_eq!(wire.into_metadata().visibility, Visibility::Public);
    }

    #[test]
    fn github_repo_into_metadata_drops_empty_description() {
        let wire = GitHubRepo {
            name:           "x".into(),
            owner:          GitHubOwner {
                login: "o".into(),
            },
            description:    Some(String::new()),
            default_branch: "main".into(),
            private:        false,
            visibility:     Some("public".into()),
            archived:       false,
            clone_url:      String::new(),
            topics:         Vec::new(),
        };
        assert!(wire.into_metadata().description.is_none());
    }

    #[test]
    fn github_create_body_maps_visibility_to_private() {
        let spec = CreateRepoSpec {
            name:           "homma".into(),
            description:    Some("hi".into()),
            visibility:     Visibility::Private,
            owner_kind:     OwnerKind::User,
            default_branch: Some("dev".into()),
            auto_init:      false,
        };
        let body = GitHubCreateBody::from_spec(&spec);
        assert!(body.private);
        assert!(!body.auto_init);
    }

    #[test]
    fn github_create_body_internal_maps_to_private_true() {
        let mut spec = CreateRepoSpec::new("x");
        spec.visibility = Visibility::Internal;
        let body = GitHubCreateBody::from_spec(&spec);
        assert!(body.private);
    }

    #[test]
    fn github_create_body_public_is_private_false() {
        let spec = CreateRepoSpec::new("x");
        let body = GitHubCreateBody::from_spec(&spec);
        assert!(!body.private);
    }

    #[test]
    fn github_create_body_serializes_without_default_branch() {
        // GitHub rejects default_branch in the create body. Ensure we never
        // send it, regardless of what the spec carries.
        let spec = CreateRepoSpec {
            name:           "homma".into(),
            description:    None,
            visibility:     Visibility::Public,
            owner_kind:     OwnerKind::User,
            default_branch: Some("dev".into()),
            auto_init:      false,
        };
        let body = GitHubCreateBody::from_spec(&spec);
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("default_branch"), "got: {json}");
        assert!(!json.contains("null"), "got: {json}");
    }

    #[test]
    fn github_patch_body_serializes_only_set_fields() {
        let body = GitHubPatchBody {
            archived:       Some(true),
            default_branch: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"archived\":true"));
        assert!(!json.contains("default_branch"));
    }

    #[test]
    fn github_patch_body_can_carry_default_branch_only() {
        let body = GitHubPatchBody {
            archived:       None,
            default_branch: Some("dev".into()),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"default_branch\":\"dev\""));
        assert!(!json.contains("archived"));
    }

    #[test]
    fn github_repo_deserializes_realistic_payload() {
        // Trimmed shape of a real GET /repos/orgrinrt/homma response. Sanity
        // check that serde tolerates the long tail of fields we don't use.
        let payload = r#"{
            "id": 123456789,
            "node_id": "R_kgABCDEF",
            "name": "homma",
            "full_name": "orgrinrt/homma",
            "owner": {
                "login": "orgrinrt",
                "id": 1234,
                "node_id": "MDQ6VXNlcjEyMzQ=",
                "type": "User"
            },
            "private": false,
            "html_url": "https://github.com/orgrinrt/homma",
            "description": "workspace tooling",
            "fork": false,
            "default_branch": "dev",
            "visibility": "public",
            "archived": false,
            "disabled": false,
            "clone_url": "https://github.com/orgrinrt/homma.git",
            "ssh_url": "git@github.com:orgrinrt/homma.git",
            "topics": ["rust", "workspace"],
            "stargazers_count": 0,
            "watchers_count": 0,
            "forks_count": 0,
            "size": 4096
        }"#;
        let wire: GitHubRepo = serde_json::from_str(payload).unwrap();
        let m = wire.into_metadata();
        assert_eq!(m.name, "homma");
        assert_eq!(m.default_branch, "dev");
        assert_eq!(m.visibility, Visibility::Public);
        assert_eq!(m.topics, vec!["rust".to_string(), "workspace".into()]);
    }
}

#[cfg(test)]
mod redirect_tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    use super::*;
    use crate::forge::Forge;

    /// A stub of the one GitHub behaviour that matters here: a renamed repo
    /// answers `301` to a new path, and that path is private, so it answers
    /// `404` to anyone without credentials and `200` to anyone with them.
    ///
    /// Returns the base url. The thread ends when the listener is dropped
    /// after the last request, which is bounded because each test makes a
    /// fixed number.
    fn renamed_private_repo(requests: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for _ in 0 .. requests {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(sock.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut authorized = false;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap() == 0 {
                        break;
                    }
                    if header.trim().is_empty() {
                        break;
                    }
                    if header.to_ascii_lowercase().starts_with("authorization:") {
                        authorized = true;
                    }
                }
                let response = if request_line.contains("/repos/o/renamed") {
                    "HTTP/1.1 301 Moved Permanently\r\nLocation: /repositories/123\r\n\
                     Content-Length: 0\r\n\r\n"
                        .to_string()
                } else if request_line.contains("/repositories/123") {
                    if authorized {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_string()
                    } else {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                    }
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                };
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
            }
        });
        url
    }

    #[test]
    fn a_renamed_private_repo_is_still_found_across_the_redirect() {
        // The failure this pins: ureq defaults to dropping `Authorization` on
        // any redirect, GitHub answers 301 for a renamed repo, and the
        // followed request then arrives anonymous. A private repo answers 404
        // to that, `repo_exists` maps 404 to `Ok(false)`, and `verify --forge`
        // reports a repo that exists as absent. A rename is the normal reason
        // an owner or name goes stale, which is the case the check exists for.
        let url = renamed_private_repo(2);
        let client = GitHubClient::with_token(&url, "t");
        assert_eq!(
            client.repo_exists("o", "renamed").unwrap(),
            true,
            "the credential was dropped following the redirect, so a repo that \
             exists was reported absent"
        );
    }

    #[test]
    fn the_stub_answers_absent_without_a_credential() {
        // The control. Without it the test above passes for a stub that says
        // 200 to everyone, which would prove nothing about the header at all.
        let url = renamed_private_repo(2);
        let client = GitHubClient::anonymous(&url);
        assert_eq!(client.repo_exists("o", "renamed").unwrap(), false);
    }
}
