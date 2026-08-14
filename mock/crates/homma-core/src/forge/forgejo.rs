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
use super::{CreateRepoSpec, Forge, ForgeError, OwnerKind, RepoMetadata, Visibility};
use crate::config::ForgeConfig;

/// A [`Forge`] backed by the Forgejo / Gitea REST API.
pub struct ForgejoClient {
    api_url: String,
    token: Option<String>,
    agent: ureq::Agent,
}

impl ForgejoClient {
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
            }
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
    /// sets `internal = true` once the repo exists. The migrate command
    /// (#452) handles this when source-side visibility is Internal.
    fn create_repo(&self, owner: &str, spec: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
        let url = self.create_path(owner, spec.owner_kind);
        let body = ForgejoCreateBody::from_spec(spec);
        match self.post_json(&url, &body) {
            Ok(resp) => {
                let wire: ForgejoRepo = resp.into_json().map_err(box_backend)?;
                Ok(wire.into_metadata())
            }
            Err(ureq::Error::Status(409, _)) => Err(ForgeError::RepoAlreadyExists {
                owner: owner.into(),
                name: spec.name.clone(),
            }),
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
}

/// Wire shape of `GET /repos/{owner}/{name}` (and the response of create-repo
/// and patch). Captures the migrate-relevant fields; ignores the rest.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ForgejoRepo {
    pub name: String,
    pub owner: ForgejoOwner,
    pub description: Option<String>,
    pub default_branch: String,
    pub private: bool,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub archived: bool,
    pub clone_url: String,
    #[serde(default)]
    pub topics: Vec<String>,
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
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub auto_init: bool,
}

impl ForgejoCreateBody {
    pub(crate) fn from_spec(spec: &CreateRepoSpec) -> Self {
        Self {
            name: spec.name.clone(),
            description: spec.description.clone(),
            private: matches!(spec.visibility, Visibility::Private | Visibility::Internal),
            default_branch: spec.default_branch.clone(),
            auto_init: spec.auto_init,
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
        }
        ureq::Error::Transport(t) => ForgeError::Backend(Box::new(t)),
    }
}

/// Pure mapping from `(status, body)` to a `ForgeError`. Lifted out so tests
/// can exercise the error matrix without spinning up a real client.
pub(crate) fn map_status(status: u16, owner: &str, name: &str, body: String) -> ForgeError {
    match status {
        404 => ForgeError::RepoNotFound {
            owner: owner.into(),
            name: name.into(),
        },
        401 | 403 => ForgeError::Unauthorized { reason: body },
        409 => ForgeError::RepoAlreadyExists {
            owner: owner.into(),
            name: name.into(),
        },
        _ => ForgeError::UnexpectedStatus {
            status,
            body: truncate(body, 512),
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
mod tests {
    use super::*;
    use crate::config::ForgeKind;

    fn cfg() -> ForgeConfig {
        ForgeConfig {
            kind: ForgeKind::Forgejo,
            base_url: "https://codeberg.org".into(),
            api_url: "https://codeberg.org/api/v1".into(),
            token_env: None,
        }
    }

    #[test]
    fn create_path_user_owner() {
        let c = ForgejoClient::anonymous("https://codeberg.org/api/v1");
        assert_eq!(
            c.create_path("orgrinrt", OwnerKind::User),
            "https://codeberg.org/api/v1/user/repos"
        );
    }

    #[test]
    fn create_path_org_owner() {
        let c = ForgejoClient::anonymous("https://codeberg.org/api/v1");
        assert_eq!(
            c.create_path("hiisi", OwnerKind::Org),
            "https://codeberg.org/api/v1/orgs/hiisi/repos"
        );
    }

    #[test]
    fn repo_path_composes_api_url() {
        let c = ForgejoClient::anonymous("https://codeberg.org/api/v1");
        assert_eq!(
            c.repo_path("orgrinrt", "homma"),
            "https://codeberg.org/api/v1/repos/orgrinrt/homma"
        );
    }

    #[test]
    fn new_reads_token_from_env() {
        let var = "HOMMA_TEST_CODEBERG_TOKEN";
        std::env::set_var(var, "secret");
        let mut f = cfg();
        f.token_env = Some(var.into());
        let c = ForgejoClient::new(&f);
        assert_eq!(c.token.as_deref(), Some("secret"));
        std::env::remove_var(var);
    }

    #[test]
    fn new_handles_missing_token_env() {
        let mut f = cfg();
        f.token_env = Some("HOMMA_TEST_DEFINITELY_UNSET_VAR".into());
        let c = ForgejoClient::new(&f);
        assert!(c.token.is_none());
    }

    #[test]
    fn auth_header_present_when_token_set() {
        let c = ForgejoClient::with_token("https://codeberg.org/api/v1", "abc");
        let (k, v) = c.auth_header().unwrap();
        assert_eq!(k, "Authorization");
        assert_eq!(v, "token abc");
    }

    #[test]
    fn auth_header_absent_when_anonymous() {
        let c = ForgejoClient::anonymous("https://codeberg.org/api/v1");
        assert!(c.auth_header().is_none());
    }

    #[test]
    fn map_status_404_is_repo_not_found() {
        let e = map_status(404, "orgrinrt", "homma", "not found".into());
        assert!(matches!(e, ForgeError::RepoNotFound { .. }));
    }

    #[test]
    fn map_status_401_403_are_unauthorized() {
        assert!(matches!(
            map_status(401, "o", "n", "bad token".into()),
            ForgeError::Unauthorized { .. }
        ));
        assert!(matches!(
            map_status(403, "o", "n", "forbidden".into()),
            ForgeError::Unauthorized { .. }
        ));
    }

    #[test]
    fn map_status_409_is_repo_already_exists() {
        let e = map_status(409, "orgrinrt", "homma", "conflict".into());
        assert!(matches!(e, ForgeError::RepoAlreadyExists { .. }));
    }

    #[test]
    fn map_status_other_carries_status_and_body() {
        let e = map_status(502, "o", "n", "bad gateway".into());
        match e {
            ForgeError::UnexpectedStatus { status, body } => {
                assert_eq!(status, 502);
                assert!(body.contains("bad gateway"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn map_status_truncates_long_body() {
        let big = "x".repeat(2048);
        let e = map_status(500, "o", "n", big);
        match e {
            ForgeError::UnexpectedStatus { body, .. } => {
                assert!(body.len() <= 512 + "... [truncated]".len());
                assert!(body.ends_with("... [truncated]"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn map_status_truncates_multibyte_safely() {
        // Regression: String::truncate panics if the byte index does not
        // fall on a UTF-8 char boundary. `ä` is two bytes; 300 copies put
        // many `ä` starts on odd byte offsets, so the 512-byte cap often
        // lands mid-character. Must walk back to the nearest boundary.
        let big = "ä".repeat(300);
        let e = map_status(500, "o", "n", big);
        match e {
            ForgeError::UnexpectedStatus { body, .. } => {
                assert!(body.ends_with("... [truncated]"));
                // Truncated body is still valid UTF-8: re-parsing succeeds.
                assert!(std::str::from_utf8(body.as_bytes()).is_ok());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn forgejo_repo_into_metadata_public() {
        let wire = ForgejoRepo {
            name: "homma".into(),
            owner: ForgejoOwner {
                login: "orgrinrt".into(),
            },
            description: Some("workspace tooling".into()),
            default_branch: "dev".into(),
            private: false,
            internal: false,
            archived: false,
            clone_url: "https://codeberg.org/orgrinrt/homma.git".into(),
            topics: vec!["rust".into()],
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
    fn forgejo_repo_into_metadata_private_wins_over_internal() {
        let wire = ForgejoRepo {
            name: "x".into(),
            owner: ForgejoOwner { login: "o".into() },
            description: None,
            default_branch: "main".into(),
            private: true,
            internal: true,
            archived: false,
            clone_url: String::new(),
            topics: Vec::new(),
        };
        assert_eq!(wire.into_metadata().visibility, Visibility::Private);
    }

    #[test]
    fn forgejo_repo_into_metadata_internal_only() {
        let wire = ForgejoRepo {
            name: "x".into(),
            owner: ForgejoOwner { login: "o".into() },
            description: None,
            default_branch: "main".into(),
            private: false,
            internal: true,
            archived: false,
            clone_url: String::new(),
            topics: Vec::new(),
        };
        assert_eq!(wire.into_metadata().visibility, Visibility::Internal);
    }

    #[test]
    fn forgejo_repo_into_metadata_drops_empty_description() {
        let wire = ForgejoRepo {
            name: "x".into(),
            owner: ForgejoOwner { login: "o".into() },
            description: Some(String::new()),
            default_branch: "main".into(),
            private: false,
            internal: false,
            archived: false,
            clone_url: String::new(),
            topics: Vec::new(),
        };
        assert!(wire.into_metadata().description.is_none());
    }

    #[test]
    fn forgejo_create_body_maps_visibility_to_private() {
        let spec = CreateRepoSpec {
            name: "homma".into(),
            description: Some("hi".into()),
            visibility: Visibility::Private,
            owner_kind: OwnerKind::User,
            default_branch: Some("dev".into()),
            auto_init: false,
        };
        let body = ForgejoCreateBody::from_spec(&spec);
        assert!(body.private);
        assert_eq!(body.default_branch.as_deref(), Some("dev"));
        assert!(!body.auto_init);
    }

    #[test]
    fn forgejo_create_body_internal_maps_to_private_true() {
        // Forgejo's create endpoint has no separate `internal` field; the
        // client maps Internal to `private: true`. Org-internal repos need
        // a separate PATCH if the host supports it.
        let mut spec = CreateRepoSpec::new("x");
        spec.visibility = Visibility::Internal;
        let body = ForgejoCreateBody::from_spec(&spec);
        assert!(body.private);
    }

    #[test]
    fn forgejo_create_body_public_is_private_false() {
        let spec = CreateRepoSpec::new("x");
        let body = ForgejoCreateBody::from_spec(&spec);
        assert!(!body.private);
    }

    #[test]
    fn forgejo_create_body_serializes_without_nulls() {
        let spec = CreateRepoSpec::new("homma");
        let body = ForgejoCreateBody::from_spec(&spec);
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("null"), "got: {json}");
        assert!(!json.contains("description"));
        assert!(!json.contains("default_branch"));
    }

    #[test]
    fn forgejo_repo_deserializes_realistic_payload() {
        // Captured shape of GET /repos/orgrinrt/homma response (minus fields
        // we don't use). Sanity check that serde can parse a full payload.
        let payload = r#"{
            "id": 12345,
            "name": "homma",
            "full_name": "orgrinrt/homma",
            "owner": { "id": 7, "login": "orgrinrt", "username": "orgrinrt" },
            "description": "workspace tooling",
            "default_branch": "dev",
            "private": false,
            "internal": false,
            "archived": false,
            "clone_url": "https://codeberg.org/orgrinrt/homma.git",
            "ssh_url": "git@codeberg.org:orgrinrt/homma.git",
            "html_url": "https://codeberg.org/orgrinrt/homma",
            "topics": ["rust", "workspace"],
            "size": 4096,
            "stars_count": 0
        }"#;
        let wire: ForgejoRepo = serde_json::from_str(payload).unwrap();
        let m = wire.into_metadata();
        assert_eq!(m.name, "homma");
        assert_eq!(m.topics, vec!["rust".to_string(), "workspace".into()]);
    }
}
