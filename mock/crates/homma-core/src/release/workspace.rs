//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a repo releases as: one version in its root manifest, or a virtual
//! cargo workspace whose members carry their own, released as a set with a
//! primary whose version and tags are the release's and riders that publish
//! when their manifests moved.

use std::fmt;
use std::path::Path;

use homma_api::{RepoKind, Version};

use super::git::{self, GitError};
use super::packages;

/// A member of a virtual workspace: its package name and its manifest,
/// relative to the root with `/` between parts, as git names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name:     String,
    pub manifest: String,
}

/// How a repo's version is held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    /// One version, in the root manifests.
    Single,
    /// A virtual workspace: the primary's version is the release's, and every
    /// other publishable member rides.
    Set {
        primary: Member,
        riders:  Vec<Member>,
    },
}

/// A virtual workspace whose primary cannot be told: no member is named as
/// the repo and there is more than one, or there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoPrimary {
    pub repo:    String,
    pub members: Vec<String>,
}

impl fmt::Display for NoPrimary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a virtual workspace releases as its member named `{}`, and its publishable members are \
             {}",
            self.repo,
            if self.members.is_empty() {
                "none".to_string()
            } else {
                self.members.join(", ")
            }
        )
    }
}

impl std::error::Error for NoPrimary {}

/// Whether the root manifest at `root` is a virtual workspace: a
/// `[workspace]` with no `[package]` and no `[workspace.package] version`.
pub fn is_virtual(text: &str) -> bool {
    let Ok(doc) = toml::from_str::<toml::Value>(text) else {
        return false;
    };
    let ws = doc.get("workspace");
    ws.is_some()
        && doc.get("package").is_none()
        && ws
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .is_none()
}

/// The layout at `root`. The primary of a set is the member named as the
/// root's directory, else the one publishable member.
pub fn layout(root: &Path, kind: RepoKind) -> Result<Layout, NoPrimary> {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    if !kind.has_crate() || !is_virtual(&text) {
        return Ok(Layout::Single);
    }
    let repo = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut members: Vec<Member> = packages::members(root)
        .into_iter()
        .map(|(name, manifest)| {
            Member {
                name,
                manifest,
            }
        })
        .collect();
    let at = members
        .iter()
        .position(|m| m.name == repo)
        .or_else(|| (members.len() == 1).then_some(0));
    match at {
        Some(i) => {
            let primary = members.remove(i);
            Ok(Layout::Set {
                primary,
                riders: members,
            })
        },
        None => {
            Err(NoPrimary {
                repo,
                members: members.into_iter().map(|m| m.name).collect(),
            })
        },
    }
}

/// `[package] version` of a member manifest's text.
pub fn package_version(text: &str) -> Option<Version> {
    let doc = toml::from_str::<toml::Value>(text).ok()?;
    doc.get("package")?.get("version")?.as_str()?.parse().ok()
}

/// The version a member's manifest carries at `rev`, or none where it is not
/// there or carries none.
pub fn version_at(root: &Path, rev: &str, manifest: &str) -> Result<Option<Version>, GitError> {
    Ok(git::show(root, rev, manifest)?.and_then(|t| package_version(&t)))
}

/// The version a member's manifest carries in the tree.
pub fn version_in_tree(root: &Path, member: &Member) -> Option<Version> {
    std::fs::read_to_string(root.join(&member.manifest))
        .ok()
        .and_then(|t| package_version(&t))
}

/// Write `version` into a member's `[package] version`, in place.
pub fn write_version(root: &Path, member: &Member, version: &Version) -> std::io::Result<()> {
    let path = root.join(&member.manifest);
    let text = std::fs::read_to_string(&path)?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| std::io::Error::other(e.to_string()))?;
    let item = doc
        .get_mut("package")
        .and_then(|p| p.get_mut("version"))
        .ok_or_else(|| std::io::Error::other(format!("{}: no version", member.manifest)))?;
    let decor = item.as_value().map(|v| v.decor().clone());
    let mut value = toml_edit::Value::from(version.to_string());
    if let Some(d) = decor {
        *value.decor_mut() = d;
    }
    *item = toml_edit::Item::Value(value);
    std::fs::write(&path, doc.to_string())
}
