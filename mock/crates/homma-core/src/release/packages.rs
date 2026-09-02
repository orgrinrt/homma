//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a repo publishes and what the registries say about it: the package
//! names read off the manifests, the tag convention, and the published
//! versions fetched once so the check and the plan read the same answer.

use std::collections::BTreeMap;
use std::path::Path;

use homma_api::{RepoKind, Version};

use super::registry::{self, Registry};

/// What the registries answered, so the check reads them once and a test can
/// hand in canned answers.
#[derive(Debug, Default, Clone)]
pub struct Published {
    /// Per package name on each registry, in publish order.
    pub versions: BTreeMap<(Registry, String), Vec<Version>>,
}

impl Published {
    pub fn get(&self, registry: Registry, package: &str) -> Option<&Vec<Version>> {
        self.versions.get(&(registry, package.to_string()))
    }
}

/// The packages a repo ships, by registry, read off its manifests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Packages {
    pub crates: Vec<String>,
    pub jsr:    Option<String>,
    pub npm:    Option<String>,
}

impl Packages {
    /// Every registry and package name this repo ships to.
    pub fn each(&self) -> Vec<(Registry, String)> {
        let mut out: Vec<(Registry, String)> = self
            .crates
            .iter()
            .map(|c| (Registry::CratesIo, c.clone()))
            .collect();
        if let Some(j) = &self.jsr {
            out.push((Registry::Jsr, j.clone()));
        }
        if let Some(n) = &self.npm {
            out.push((Registry::Npm, n.clone()));
        }
        out
    }
}

/// Read the packages off the manifests: every publishable crate the
/// workspace names, the jsr name in `deno.json`, and the npm name in a root
/// `package.json` where one is kept.
pub fn packages(root: &Path, repo_kind: RepoKind) -> Packages {
    let mut out = Packages::default();
    if repo_kind.has_crate() {
        out.crates = crate_names(root);
    }
    if repo_kind.has_deno() {
        if let Ok(text) = std::fs::read_to_string(root.join("deno.json")) {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) {
                out.jsr = doc.get("name").and_then(|n| n.as_str()).map(str::to_string);
            }
        }
        if let Ok(text) = std::fs::read_to_string(root.join("package.json")) {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) {
                out.npm = doc.get("name").and_then(|n| n.as_str()).map(str::to_string);
            }
        }
    }
    out
}

fn crate_names(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(doc) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    let publishable = |pkg: &toml::Value| {
        pkg.get("publish")
            .map(|p| p.as_bool() != Some(false))
            .unwrap_or(true)
    };
    let mut names = Vec::new();
    if let Some(pkg) = doc.get("package") {
        if publishable(pkg) {
            if let Some(n) = pkg.get("name").and_then(|n| n.as_str()) {
                names.push(n.to_string());
            }
        }
    }
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for m in members.iter().filter_map(|m| m.as_str()) {
        for dir in expand_member(root, m) {
            let Ok(t) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
                continue;
            };
            let Ok(d) = toml::from_str::<toml::Value>(&t) else { continue };
            let Some(pkg) = d.get("package") else { continue };
            if !publishable(pkg) {
                continue;
            }
            if let Some(n) = pkg.get("name").and_then(|n| n.as_str()) {
                if !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
        }
    }
    names
}

/// A member entry is a path or a path ending in `/*`.
fn expand_member(root: &Path, member: &str) -> Vec<std::path::PathBuf> {
    match member.strip_suffix("/*") {
        Some(parent) => {
            let mut dirs: Vec<_> = std::fs::read_dir(root.join(parent))
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect()
                })
                .unwrap_or_default();
            dirs.sort();
            dirs
        },
        None => vec![root.join(member)],
    }
}

/// The name of the tag for `version`, following the repo's own convention:
/// `v` where its tags carry one, bare where they do not, `v` where it has
/// none yet.
pub fn tag_name(tags: &[String], version: &Version) -> String {
    let bare = tags.iter().any(|t| t.parse::<Version>().is_ok());
    let prefixed = tags.iter().any(|t| {
        t.strip_prefix('v')
            .is_some_and(|r| r.parse::<Version>().is_ok())
    });
    if bare && !prefixed { version.to_string() } else { format!("v{version}") }
}

/// The version a tag names, with or without a `v`.
pub fn tag_version(tag: &str) -> Option<Version> {
    tag.strip_prefix('v').unwrap_or(tag).parse().ok()
}

/// Ask every registry the repo ships to.
pub fn fetch_published(packages: &Packages) -> Result<Published, registry::Unreachable> {
    let mut out = Published::default();
    for (reg, name) in packages.each() {
        let v = registry::published_versions(reg, &name)?;
        out.versions.insert((reg, name), v);
    }
    Ok(out)
}
