//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The estate: every package the workspace's repos publish, what each
//! manifest said at each tag, what the registry holds, and where the two
//! disagree. Both ecosystems, the same questions asked of each.

use std::path::{Path, PathBuf};

use homma_api::Version;
use serde::Serialize;

use super::git::{self, GitError};
use super::registry::{Holding, Registry, Unreachable};
use super::{packages, sequence};

/// One package a repo declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Package {
    #[serde(serialize_with = "as_key")]
    pub registry: Registry,
    pub name:     String,
    /// The manifest declaring it, relative to the repo root.
    pub manifest: String,
    /// Its version in the working tree, where it names one.
    pub version:  Option<String>,
}

fn as_key<S: serde::Serializer>(r: &Registry, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(r.key())
}

/// The jsr package `deno.json` declares, where its scope is `scope`, or any
/// scope where none is given.
pub fn jsr_packages(repo: &Path, scope: Option<&str>) -> Vec<Package> {
    let Ok(text) = std::fs::read_to_string(repo.join("deno.json")) else {
        return Vec::new();
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(name) = doc.get("name").and_then(|n| n.as_str()) else {
        return Vec::new();
    };
    if let Some(scope) = scope {
        let ours = name
            .strip_prefix('@')
            .and_then(|n| n.split_once('/'))
            .is_some_and(|(s, _)| s == scope.trim_start_matches('@'));
        if !ours {
            return Vec::new();
        }
    }
    vec![Package {
        registry: Registry::Jsr,
        name:     name.to_string(),
        manifest: "deno.json".into(),
        version:  doc
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }]
}

/// Every cargo crate not marked `publish = false`.
pub fn cargo_packages(repo: &Path) -> Vec<Package> {
    packages::crates(repo)
        .into_iter()
        .map(|c| {
            Package {
                registry: Registry::CratesIo,
                name:     c.name,
                manifest: c.manifest,
                version:  c.version,
            }
        })
        .collect()
}

/// Every package under `root`, read only from directories that are
/// repositories, jsr first then cargo within one, repos by name.
pub fn packages_under(root: &Path, scope: Option<&str>) -> Vec<(PathBuf, Package)> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.sort();
    let mut out = Vec::new();
    for d in dirs.into_iter().filter(|d| d.join(".git").exists()) {
        for p in jsr_packages(&d, scope)
            .into_iter()
            .chain(cargo_packages(&d))
        {
            out.push((d.clone(), p));
        }
    }
    out
}

/// The repo's release tags for `registry`, as `(tag, version)` in version
/// order: jsr reads `v`-prefixed tags and cargo bare ones, and a tag that is
/// neither is no release tag.
pub fn tag_versions(tags: &[String], registry: Registry) -> Vec<(String, Version)> {
    let mut out: Vec<(String, Version)> = tags
        .iter()
        .filter_map(|t| {
            let bare = match registry {
                Registry::Jsr => t.strip_prefix('v')?,
                _ => t.as_str(),
            };
            if !bare.starts_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            bare.parse::<Version>().ok().map(|v| (t.clone(), v))
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// The version `pkg`'s own manifest carried at `tag`, read out of the tag
/// and never the tree: a member inheriting reads the root of the same tag.
pub fn declared_at(repo: &Path, tag: &str, pkg: &Package) -> Result<Option<String>, GitError> {
    let Some(text) = git::show(repo, tag, &pkg.manifest)? else {
        return Ok(None);
    };
    if pkg.registry == Registry::Jsr {
        return Ok(serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|d| d.get("version")?.as_str().map(str::to_string)));
    }
    let Ok(doc) = toml::from_str::<toml::Value>(&text) else {
        return Ok(None);
    };
    match doc.get("package").and_then(|p| p.get("version")) {
        Some(toml::Value::String(v)) => Ok(Some(v.clone())),
        Some(toml::Value::Table(t))
            if t.get("workspace").and_then(|w| w.as_bool()) == Some(true) =>
        {
            let Some(root) = git::show(repo, tag, "Cargo.toml")? else {
                return Ok(None);
            };
            Ok(toml::from_str::<toml::Value>(&root).ok().and_then(|r| {
                r.get("workspace")?
                    .get("package")?
                    .get("version")?
                    .as_str()
                    .map(str::to_string)
            }))
        },
        _ => Ok(None),
    }
}

/// The steps of a published line that are not one legal increment, as
/// `low -> high`; pre-releases take no part.
pub fn jumps(versions: &[Version]) -> Vec<String> {
    sequence::gaps(versions)
        .into_iter()
        .map(|(a, b)| format!("{a} -> {b}"))
        .collect()
}

/// One tag, as the estate reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagRow {
    pub tag:     String,
    pub sha:     String,
    /// What the manifest said at the tag.
    pub version: Option<String>,
    pub agrees:  bool,
}

/// One package's line in the estate.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub package:     Package,
    pub repo:        String,
    pub tags:        Vec<TagRow>,
    pub published:   Vec<String>,
    /// The highest live release.
    pub latest:      Option<String>,
    /// Tagged versions the registry does not hold, live or yanked.
    pub missing:     Vec<String>,
    pub jumps:       Vec<String>,
    /// Tags whose manifest disagrees with the tag's own version.
    pub disagreeing: Vec<String>,
    /// Paths `git status` reports, staged, unstaged or untracked.
    pub dirty:       usize,
}

/// Measure one package: its tags against its manifest, and against what
/// `holding` says the registry has.
pub fn measure(repo: &Path, pkg: Package, holding: &Holding) -> Result<Row, GitError> {
    let tags = git::tags(repo)?;
    let mut rows = Vec::new();
    for (tag, version) in tag_versions(&tags, pkg.registry) {
        let declared = declared_at(repo, &tag, &pkg)?;
        rows.push(TagRow {
            sha: git::tag_target(repo, &tag)?,
            agrees: declared.as_deref() == Some(version.to_string().as_str()),
            version: declared,
            tag,
        });
    }
    let mut live = holding.live.clone();
    live.sort();
    let missing = rows
        .iter()
        .filter_map(|t| t.version.clone())
        .filter(|v| {
            v.parse::<Version>()
                .map(|v| !holding.live.contains(&v) && !holding.yanked.contains(&v))
                .unwrap_or(true)
        })
        .collect();
    let dirty = git::trimmed(repo, &["status", "--porcelain"])?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    Ok(Row {
        repo: repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        disagreeing: rows
            .iter()
            .filter(|t| !t.agrees)
            .map(|t| t.tag.clone())
            .collect(),
        tags: rows,
        jumps: jumps(&live),
        latest: sequence::releases(&live).last().map(Version::to_string),
        published: live.iter().map(Version::to_string).collect(),
        missing,
        dirty,
        package: pkg,
    })
}

/// Measure the whole estate under `root`, asking `ask` what each registry
/// holds; a registry that cannot answer stops the measurement with its
/// reason rather than reading as nothing published.
pub fn measure_all(
    root: &Path,
    scope: Option<&str>,
    ask: impl Fn(Registry, &str) -> Result<Holding, Unreachable>,
) -> Result<Vec<Row>, EstateError> {
    let mut out = Vec::new();
    for (repo, pkg) in packages_under(root, scope) {
        let holding = ask(pkg.registry, &pkg.name).map_err(EstateError::Registry)?;
        out.push(measure(&repo, pkg, &holding).map_err(EstateError::Git)?);
    }
    Ok(out)
}

/// Why the estate could not be measured.
#[derive(Debug)]
pub enum EstateError {
    Registry(Unreachable),
    Git(GitError),
}

impl std::fmt::Display for EstateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EstateError::Registry(e) => write!(f, "{e}"),
            EstateError::Git(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EstateError {}

#[cfg(test)]
#[path = "estate_tests.rs"]
mod tests;
