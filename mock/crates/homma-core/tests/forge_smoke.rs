//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke tests for the `Forge` trait surface.
//!
//! No network. A minimal in-memory mock impl proves the trait is wireable
//! and the value types compose as the migrate command will use them.
//! Concrete clients (`ForgejoClient`, `GitHubClient`) ship in #449 / #450.

use std::cell::RefCell;
use std::collections::HashMap;

use homma_core::forge::OwnerKind;
use homma_core::{CreateRepoSpec, Forge, ForgeError, RepoMetadata, Visibility};

/// In-memory mock Forge. Stores repos in a `(owner, name) -> metadata` map.
struct MockForge {
    repos: RefCell<HashMap<(String, String), RepoMetadata>>,
}

impl MockForge {
    fn new() -> Self {
        Self {
            repos: RefCell::new(HashMap::new()),
        }
    }

    fn with_repo(self, m: RepoMetadata) -> Self {
        self.repos
            .borrow_mut()
            .insert((m.owner.clone(), m.name.clone()), m);
        self
    }
}

impl Forge for MockForge {
    fn fetch_repo(&self, owner: &str, name: &str) -> Result<RepoMetadata, ForgeError> {
        self.repos
            .borrow()
            .get(&(owner.to_string(), name.to_string()))
            .cloned()
            .ok_or_else(|| {
                ForgeError::RepoNotFound {
                    owner: owner.into(),
                    name:  name.into(),
                }
            })
    }

    fn repo_exists(&self, owner: &str, name: &str) -> Result<bool, ForgeError> {
        Ok(self
            .repos
            .borrow()
            .contains_key(&(owner.to_string(), name.to_string())))
    }

    fn create_repo(&self, owner: &str, spec: &CreateRepoSpec) -> Result<RepoMetadata, ForgeError> {
        let key = (owner.to_string(), spec.name.clone());
        let mut repos = self.repos.borrow_mut();
        if repos.contains_key(&key) {
            return Err(ForgeError::RepoAlreadyExists {
                owner: owner.into(),
                name:  spec.name.clone(),
            });
        }
        let meta = RepoMetadata {
            owner:           owner.into(),
            name:            spec.name.clone(),
            description:     spec.description.clone(),
            default_branch:  spec.default_branch.clone().unwrap_or_else(|| "main".into()),
            visibility:      spec.visibility,
            topics:          Vec::new(),
            archived:        false,
            clone_url_https: format!("https://mock.invalid/{owner}/{}.git", spec.name),
        };
        repos.insert(key, meta.clone());
        Ok(meta)
    }

    fn archive_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError> {
        let mut repos = self.repos.borrow_mut();
        match repos.get_mut(&(owner.to_string(), name.to_string())) {
            Some(m) => {
                m.archived = true;
                Ok(())
            },
            None => {
                Err(ForgeError::RepoNotFound {
                    owner: owner.into(),
                    name:  name.into(),
                })
            },
        }
    }

    fn delete_repo(&self, owner: &str, name: &str) -> Result<(), ForgeError> {
        match self
            .repos
            .borrow_mut()
            .remove(&(owner.to_string(), name.to_string()))
        {
            Some(_) => Ok(()),
            None => {
                Err(ForgeError::RepoNotFound {
                    owner: owner.into(),
                    name:  name.into(),
                })
            },
        }
    }

    /// The mock stands in for a forge that accepts whatever it is given, since
    /// no test here is about credentials.
    fn credential_works(&self) -> Result<bool, ForgeError> {
        Ok(true)
    }

    /// A status on a repo the mock does not hold is the one error the real
    /// forges answer with; on a held repo it is accepted and forgotten.
    fn set_commit_status(
        &self,
        owner: &str,
        name: &str,
        _sha: &str,
        _status: &homma_core::forge::CommitStatus,
    ) -> Result<(), ForgeError> {
        if self.repo_exists(owner, name)? {
            Ok(())
        } else {
            Err(ForgeError::RepoNotFound {
                owner: owner.into(),
                name:  name.into(),
            })
        }
    }

    fn create_release(
        &self,
        owner: &str,
        name: &str,
        _t: &str,
        _b: &str,
    ) -> Result<(), ForgeError> {
        if self.repo_exists(owner, name)? {
            Ok(())
        } else {
            Err(ForgeError::RepoNotFound {
                owner: owner.into(),
                name:  name.into(),
            })
        }
    }
}

fn sample_source() -> RepoMetadata {
    RepoMetadata {
        owner:           "orgrinrt".into(),
        name:            "homma".into(),
        description:     Some("workspace tooling".into()),
        default_branch:  "dev".into(),
        visibility:      Visibility::Public,
        topics:          vec!["rust".into(), "workspace".into()],
        archived:        false,
        clone_url_https: "https://github.com/orgrinrt/homma.git".into(),
    }
}

#[test]
fn fetch_repo_returns_metadata_when_present() {
    let f = MockForge::new().with_repo(sample_source());
    let m = f.fetch_repo("orgrinrt", "homma").expect("fetch");
    assert_eq!(m.default_branch, "dev");
    assert_eq!(m.visibility, Visibility::Public);
}

#[test]
fn fetch_repo_returns_not_found_when_absent() {
    let f = MockForge::new();
    let err = f.fetch_repo("orgrinrt", "homma").unwrap_err();
    assert!(matches!(err, ForgeError::RepoNotFound { .. }));
}

#[test]
fn repo_exists_round_trip() {
    let f = MockForge::new().with_repo(sample_source());
    assert!(f.repo_exists("orgrinrt", "homma").unwrap());
    assert!(!f.repo_exists("orgrinrt", "absent").unwrap());
}

#[test]
fn create_repo_round_trip_then_already_exists() {
    let f = MockForge::new();
    let spec = CreateRepoSpec::new("homma");
    let created = f.create_repo("orgrinrt", &spec).expect("create");
    assert_eq!(created.name, "homma");
    assert_eq!(created.default_branch, "main");
    assert_eq!(created.visibility, Visibility::Public);

    let err = f.create_repo("orgrinrt", &spec).unwrap_err();
    assert!(matches!(err, ForgeError::RepoAlreadyExists { .. }));
}

#[test]
fn replicate_from_copies_migrate_relevant_fields() {
    let source = sample_source();
    let spec = CreateRepoSpec::new("homma").replicate_from(&source);
    assert_eq!(spec.description.as_deref(), Some("workspace tooling"));
    assert_eq!(spec.visibility, Visibility::Public);
    assert_eq!(spec.default_branch.as_deref(), Some("dev"));
    assert!(!spec.auto_init, "migrate destinations must not auto-init");
}

#[test]
fn archive_then_delete() {
    let f = MockForge::new().with_repo(sample_source());
    f.archive_repo("orgrinrt", "homma").expect("archive");
    let after = f.fetch_repo("orgrinrt", "homma").unwrap();
    assert!(after.archived);

    f.delete_repo("orgrinrt", "homma").expect("delete");
    assert!(!f.repo_exists("orgrinrt", "homma").unwrap());
}

#[test]
fn archive_unknown_fails() {
    let f = MockForge::new();
    let err = f.archive_repo("orgrinrt", "homma").unwrap_err();
    assert!(matches!(err, ForgeError::RepoNotFound { .. }));
}

#[test]
fn create_repo_spec_defaults_to_user_owner() {
    let spec = CreateRepoSpec::new("homma");
    assert_eq!(spec.owner_kind, OwnerKind::User);
}

#[test]
fn create_repo_spec_in_org_flips_owner_kind() {
    let spec = CreateRepoSpec::new("homma").in_org();
    assert_eq!(spec.owner_kind, OwnerKind::Org);
}

#[test]
fn forge_error_display_carries_context() {
    let e = ForgeError::RepoNotFound {
        owner: "orgrinrt".into(),
        name:  "homma".into(),
    };
    let s = format!("{e}");
    assert!(s.contains("orgrinrt/homma"), "got: {s}");
}
