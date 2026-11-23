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
    /// Per package name on each registry, the yanked versions, which keep
    /// their slot though nothing installs them.
    pub yanked:   BTreeMap<(Registry, String), Vec<Version>>,
}

impl Published {
    /// The versions `package` has on `registry`, in publish order, or none
    /// where the registry was not asked or has no such package.
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
    let mut names = Vec::new();
    if let Some(pkg) = doc.get("package") {
        if publishable(pkg) {
            if let Some(n) = pkg.get("name").and_then(|n| n.as_str()) {
                names.push(n.to_string());
            }
        }
    }
    for (n, _) in members(root) {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    names
}

fn publishable(pkg: &toml::Value) -> bool {
    pkg.get("publish")
        .map(|p| p.as_bool() != Some(false))
        .unwrap_or(true)
}

/// Every publishable member the root manifest's `[workspace] members` names,
/// with its manifest relative to the root, `/` between parts, in member order.
pub fn members(root: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(doc) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    let entries = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<(String, String)> = Vec::new();
    for m in entries.iter().filter_map(|m| m.as_str()) {
        for dir in expand_member(root, m) {
            let Ok(t) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
                continue;
            };
            let Ok(d) = toml::from_str::<toml::Value>(&t) else { continue };
            let Some(pkg) = d.get("package") else { continue };
            if !publishable(pkg) {
                continue;
            }
            let Some(n) = pkg.get("name").and_then(|n| n.as_str()) else { continue };
            if out.iter().any(|(x, _)| x == n) {
                continue;
            }
            let rel = dir.strip_prefix(root).unwrap_or(&dir).join("Cargo.toml");
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push((n.to_string(), rel.join("/")));
        }
    }
    out
}

/// One publishable crate of a cargo repo: its name, its version where it has
/// one, a member's `version.workspace = true` read as the root's
/// `[workspace.package] version`, and its manifest relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    pub name:     String,
    pub version:  Option<String>,
    pub manifest: String,
}

/// Every crate of the repo not marked `publish = false`, the root's own
/// `[package]` first and then each member, a name read once.
pub fn crates(root: &Path) -> Vec<Crate> {
    let Some(doc) = read_toml(&root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let inherited = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let mut out: Vec<Crate> = Vec::new();
    let mut take = |pkg: &toml::Value, manifest: String| {
        if !publishable(pkg) {
            return;
        }
        let Some(name) = pkg.get("name").and_then(|n| n.as_str()) else { return };
        if out.iter().any(|c| c.name == name) {
            return;
        }
        let version = match pkg.get("version") {
            Some(toml::Value::String(v)) => Some(v.clone()),
            Some(toml::Value::Table(t))
                if t.get("workspace").and_then(|w| w.as_bool()) == Some(true) =>
            {
                inherited.clone()
            },
            _ => None,
        };
        out.push(Crate {
            name: name.to_string(),
            version,
            manifest,
        });
    };
    if let Some(pkg) = doc.get("package") {
        take(pkg, "Cargo.toml".to_string());
    }
    for manifest in member_manifests(root) {
        if let Some(pkg) = read_toml(&root.join(&manifest))
            .as_ref()
            .and_then(|d| d.get("package"))
        {
            take(pkg, manifest);
        }
    }
    out
}

/// Every manifest the root's `[workspace] members` names, globs expanded and
/// `.` left out since the root is read on its own, relative to the root.
pub fn member_manifests(root: &Path) -> Vec<String> {
    let Some(doc) = read_toml(&root.join("Cargo.toml")) else {
        return Vec::new();
    };
    let entries = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for m in entries
        .iter()
        .filter_map(|m| m.as_str())
        .filter(|m| *m != ".")
    {
        for dir in expand_member(root, m) {
            if !dir.join("Cargo.toml").is_file() {
                continue;
            }
            let rel = dir.strip_prefix(root).unwrap_or(&dir).join("Cargo.toml");
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            out.push(rel.join("/"));
        }
    }
    out
}

fn read_toml(path: &Path) -> Option<toml::Value> {
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
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
    tag_name_or(tags, version, "v")
}

/// Like `tag_name`, naming a repo with no version tag yet with `fresh` in front.
/// A tag that is no version says nothing of the style.
pub fn tag_name_or(tags: &[String], version: &Version, fresh: &str) -> String {
    if !tags.iter().any(|t| t.parse::<Version>().is_ok()) {
        return format!("{fresh}{version}");
    }
    let prefixed = tags.iter().any(|t| {
        t.strip_prefix('v')
            .is_some_and(|r| r.parse::<Version>().is_ok())
    });
    if prefixed { format!("v{version}") } else { version.to_string() }
}

/// The version a tag names, with or without a `v`.
pub fn tag_version(tag: &str) -> Option<Version> {
    tag.strip_prefix('v').unwrap_or(tag).parse().ok()
}

/// Ask every registry the repo ships to.
pub fn fetch_published(packages: &Packages) -> Result<Published, registry::Unreachable> {
    let mut out = Published::default();
    for (reg, name) in packages.each() {
        let h = registry::holding(reg, &name)?;
        out.versions.insert((reg, name.clone()), h.live);
        if !h.yanked.is_empty() {
            out.yanked.insert((reg, name), h.yanked);
        }
    }
    Ok(out)
}
