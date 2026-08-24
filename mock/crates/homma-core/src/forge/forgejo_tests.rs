//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests for [`super::forgejo`]. A sibling file rather than an inline module
//! because the parent was over the size limit with them in it, and a test
//! module is the seam a file of this shape splits at.

use super::*;
use crate::config::ForgeKind;

fn cfg() -> ForgeConfig {
    ForgeConfig {
        kind:      ForgeKind::Forgejo,
        base_url:  "https://codeberg.org".into(),
        api_url:   "https://codeberg.org/api/v1".into(),
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
    // Regression: String::truncate panics if the byte index does not
    // fall on a UTF-8 char boundary. `ä` is two bytes; 300 copies put
    // many `ä` starts on odd byte offsets, so the 512-byte cap often
    // lands mid-character. Must walk back to the nearest boundary.
    let big = "ä".repeat(300);
    let e = map_status(500, "o", "n", big);
    match e {
        ForgeError::UnexpectedStatus {
            body,
            ..
        } => {
            assert!(body.ends_with("... [truncated]"));
            // Truncated body is still valid UTF-8: re-parsing succeeds.
            assert!(std::str::from_utf8(body.as_bytes()).is_ok());
        },
        _ => panic!("wrong variant"),
    }
}

#[test]
fn forgejo_repo_into_metadata_public() {
    let wire = ForgejoRepo {
        name:           "homma".into(),
        owner:          ForgejoOwner {
            login: "orgrinrt".into(),
        },
        description:    Some("workspace tooling".into()),
        default_branch: "dev".into(),
        private:        false,
        internal:       false,
        archived:       false,
        clone_url:      "https://codeberg.org/orgrinrt/homma.git".into(),
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
fn forgejo_repo_into_metadata_private_wins_over_internal() {
    let wire = ForgejoRepo {
        name:           "x".into(),
        owner:          ForgejoOwner {
            login: "o".into(),
        },
        description:    None,
        default_branch: "main".into(),
        private:        true,
        internal:       true,
        archived:       false,
        clone_url:      String::new(),
        topics:         Vec::new(),
    };
    assert_eq!(wire.into_metadata().visibility, Visibility::Private);
}

#[test]
fn forgejo_repo_into_metadata_internal_only() {
    let wire = ForgejoRepo {
        name:           "x".into(),
        owner:          ForgejoOwner {
            login: "o".into(),
        },
        description:    None,
        default_branch: "main".into(),
        private:        false,
        internal:       true,
        archived:       false,
        clone_url:      String::new(),
        topics:         Vec::new(),
    };
    assert_eq!(wire.into_metadata().visibility, Visibility::Internal);
}

#[test]
fn forgejo_repo_into_metadata_drops_empty_description() {
    let wire = ForgejoRepo {
        name:           "x".into(),
        owner:          ForgejoOwner {
            login: "o".into(),
        },
        description:    Some(String::new()),
        default_branch: "main".into(),
        private:        false,
        internal:       false,
        archived:       false,
        clone_url:      String::new(),
        topics:         Vec::new(),
    };
    assert!(wire.into_metadata().description.is_none());
}

#[test]
fn forgejo_create_body_maps_visibility_to_private() {
    let spec = CreateRepoSpec {
        name:           "homma".into(),
        description:    Some("hi".into()),
        visibility:     Visibility::Private,
        owner_kind:     OwnerKind::User,
        default_branch: Some("dev".into()),
        auto_init:      false,
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
