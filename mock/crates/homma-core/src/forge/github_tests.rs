//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests for [`super::github`]. A sibling file rather than an inline module
//! because the parent was over the size limit with them in it, and a test
//! module is the seam a file of this shape splits at.

use super::*;
use crate::config::ForgeKind;

fn cfg() -> ForgeConfig {
    ForgeConfig {
        kind:      ForgeKind::Github,
        base_url:  "https://github.com".into(),
        api_url:   "https://api.github.com".into(),
        token_env: None,
        token_cmd: None,
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
            // The cut lands on a character boundary. `body` is a `String`, so
            // asserting it decodes as utf-8 asserts nothing: the type already
            // guarantees it and the truncation would have panicked before
            // reaching here. What is checkable is the length, which is where a
            // byte-count cut on multi-byte input goes wrong.
            assert!(
                body.len() <= 512 + "... [truncated]".len(),
                "the truncation kept more than it says it does: {} bytes",
                body.len()
            );
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
