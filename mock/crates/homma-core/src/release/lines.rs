//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A member on a version line of its own: declared by `shared-version =
//! false` in the `release.toml` of its directory, tagged `<crate>-v<version>`,
//! moving where its manifest moved since its last own tag, with its block in
//! its own `CHANGELOG.md`. The repo's own line is what lies outside them.

use std::path::Path;

use homma_api::{CheckSeverity, Finding, Version};

use super::check::Published;
use super::git::{self, GitError, Subject};
use super::plan::PlanError;
use super::registry::Registry;
use super::workspace::{self, Layout, Member};
use super::{changelog, packages};

/// One own line as a plan reads it: the member, its last own tag and the
/// version it carried there, its version now, the tag the release makes, and
/// the commits and the block since its last tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub name:      String,
    pub manifest:  String,
    pub last_tag:  Option<String>,
    pub from:      Option<Version>,
    pub to:        Version,
    pub tag:       String,
    pub commits:   Vec<Subject>,
    pub changelog: String,
}

impl Line {
    /// Whether it releases: its manifest moved since its last own tag, or it
    /// has none yet.
    pub fn moves(&self) -> bool {
        self.last_tag.is_none() || self.from.as_ref() != Some(&self.to)
    }

    /// The member's directory, relative to the root.
    pub fn dir(&self) -> &str {
        dir_of(&self.manifest)
    }
}

/// The directory of a member manifest, relative to the root; `.` for the root.
pub fn dir_of(manifest: &str) -> &str {
    manifest.rsplit_once('/').map_or(".", |(d, _)| d)
}

/// The members on lines of their own, in member order.
pub fn own(root: &Path) -> Vec<Member> {
    packages::members(root)
        .into_iter()
        .filter(|(_, manifest)| declares_own(&root.join(dir_of(manifest))))
        .map(|(name, manifest)| {
            Member {
                name,
                manifest,
            }
        })
        .collect()
}

/// Whether the `release.toml` in `dir` says `shared-version = false`.
fn declares_own(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("release.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
        .and_then(|d| d.get("shared-version").and_then(toml::Value::as_bool))
        == Some(false)
}

/// The tag of `name`'s line at `version`.
pub fn tag(name: &str, version: &Version) -> String {
    format!("{name}-v{version}")
}

/// The version a tag of `name`'s line names, or none where it is not one.
pub fn tag_version(name: &str, tag: &str) -> Option<Version> {
    tag.strip_prefix(name)?.strip_prefix("-v")?.parse().ok()
}

/// Whether `tag` is a tag of any of `names`' lines.
pub fn is_own_tag(names: &[String], tag: &str) -> bool {
    names.iter().any(|n| tag_version(n, tag).is_some())
}

/// The newest tag of `name`'s line by version, with its version.
pub fn last_tag(tags: &[String], name: &str) -> Option<(String, Version)> {
    let mut own: Vec<(String, Version)> = tags
        .iter()
        .filter_map(|t| tag_version(name, t).map(|v| (t.clone(), v)))
        .collect();
    own.sort_by(|a, b| a.1.cmp(&b.1));
    own.pop()
}

/// `layout` with the own lines taken out of a set's riders, since they ride
/// nothing; the primary a set names on a line of its own, where it is one.
pub fn strip(layout: Layout, own: &[Member]) -> (Layout, Option<String>) {
    match layout {
        Layout::Single => (Layout::Single, None),
        Layout::Set {
            primary,
            riders,
        } => {
            let own_primary = own
                .iter()
                .any(|m| m.name == primary.name)
                .then(|| primary.name.clone());
            let riders = riders
                .into_iter()
                .filter(|r| !own.iter().any(|m| m.name == r.name))
                .collect();
            (
                Layout::Set {
                    primary,
                    riders,
                },
                own_primary,
            )
        },
    }
}

/// The root lockfile, the one file outside the own lines that moving an own
/// line's version changes by itself.
const LOCK: &str = "Cargo.lock";

/// Whether `path` lies under the directory `dir`.
fn under(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The commits of `from..head`, or of everything `head` reaches with no
/// `from`, newest first, that `keep` takes by the paths each changed against
/// its first parent.
fn commits_where(
    root: &Path,
    from: Option<&str>,
    head: &str,
    keep: impl Fn(&str, &[String]) -> Result<bool, GitError>,
) -> Result<Vec<Subject>, GitError> {
    let all = match from {
        Some(f) => git::subjects(root, f, head)?,
        None => git::subjects_to(root, head)?,
    };
    let mut out = Vec::new();
    for s in all {
        if keep(&s.sha, &git::changed_paths(root, &s.sha)?)? {
            out.push(s);
        }
    }
    Ok(out)
}

/// The repo's own line's commits: each that changed a path under no own
/// line's directory, the root lockfile counting only where it changed a
/// package on no own line.
pub fn repo_commits(
    root: &Path,
    own: &[Member],
    from: Option<&str>,
    head: &str,
) -> Result<Vec<Subject>, GitError> {
    commits_where(root, from, head, |sha, paths| {
        let rest: Vec<&String> = paths
            .iter()
            .filter(|p| !own.iter().any(|m| under(p, dir_of(&m.manifest))))
            .collect();
        if rest.iter().any(|p| p.as_str() != LOCK) {
            return Ok(true);
        }
        if rest.is_empty() {
            return Ok(false);
        }
        lock_changes_repo(root, sha, own)
    })
}

/// An own line's commits: each that changed a path under its directory.
fn own_commits(
    root: &Path,
    dir: &str,
    from: Option<&str>,
    head: &str,
) -> Result<Vec<Subject>, GitError> {
    commits_where(root, from, head, |_, paths| {
        Ok(paths.iter().any(|p| under(p, dir)))
    })
}

/// Whether `sha`'s change to the root lockfile changed a package on no own
/// line: the lock before and after, with the own lines' packages and every
/// reference to them taken out, differ. A lock that does not read counts as
/// the repo's.
fn lock_changes_repo(root: &Path, sha: &str, own: &[Member]) -> Result<bool, GitError> {
    if git::parent_count(root, sha)? == 0 {
        return Ok(true);
    }
    let names: Vec<&str> = own.iter().map(|m| m.name.as_str()).collect();
    let rest = |text: Option<String>| -> Option<Vec<toml::Value>> {
        let doc = toml::from_str::<toml::Value>(&text?).ok()?;
        let packages = doc.get("package")?.as_array()?;
        Some(
            packages
                .iter()
                .filter(|p| {
                    !p.get("name")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|n| names.contains(&n))
                })
                .map(|p| {
                    let mut p = p.clone();
                    if let Some(deps) = p
                        .get_mut("dependencies")
                        .and_then(toml::Value::as_array_mut)
                    {
                        deps.retain(|d| {
                            !d.as_str()
                                .and_then(|d| d.split(' ').next())
                                .is_some_and(|n| names.contains(&n))
                        });
                    }
                    p
                })
                .collect(),
        )
    };
    let before = rest(git::show(root, &format!("{sha}^1"), LOCK)?);
    let after = rest(git::show(root, sha, LOCK)?);
    Ok(match (before, after) {
        (Some(b), Some(a)) => b != a,
        _ => true,
    })
}

/// Each own line at `head`, with its block dated `date`.
pub fn read(root: &Path, own: &[Member], head: &str, date: &str) -> Result<Vec<Line>, ReadError> {
    let tags = git::tags(root)?;
    let mut out = Vec::new();
    for m in own {
        let to = workspace::version_in_tree(root, m)
            .ok_or_else(|| ReadError::NoVersion(m.name.clone()))?;
        let last = last_tag(&tags, &m.name);
        let dir = dir_of(&m.manifest);
        let (from, commits) = match &last {
            Some((t, _)) => {
                (
                    workspace::version_at(root, t, &m.manifest)?,
                    own_commits(root, dir, Some(t), head)?,
                )
            },
            None => (None, own_commits(root, dir, None, head)?),
        };
        out.push(Line {
            name: m.name.clone(),
            manifest: m.manifest.clone(),
            last_tag: last.map(|(t, _)| t),
            from,
            tag: tag(&m.name, &to),
            changelog: changelog::block(&to, date, &commits),
            to,
            commits,
        });
    }
    Ok(out)
}

/// Why the own lines could not be read: git, or a member with no version.
#[derive(Debug)]
pub enum ReadError {
    Git(GitError),
    NoVersion(String),
}

impl From<GitError> for ReadError {
    fn from(e: GitError) -> Self {
        ReadError::Git(e)
    }
}

/// Every finding about the own lines against their own tags: each version a
/// registry holds has an own tag, the newest own tag is published, and the
/// working version is not below what is published.
pub fn findings(
    root: &Path,
    own: &[Member],
    tags: &[String],
    published: &Published,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for m in own {
        let versions = published
            .get(Registry::CratesIo, &m.name)
            .cloned()
            .unwrap_or_default();
        let tagged: Vec<Version> = tags
            .iter()
            .filter_map(|t| tag_version(&m.name, t))
            .collect();
        for v in &versions {
            if !tagged.contains(v) {
                out.push(Finding::new(
                    "reg.orphan",
                    CheckSeverity::Error,
                    format!(
                        "{} {v} is on crates-io with no `{}` tag",
                        m.name,
                        tag(&m.name, v)
                    ),
                ));
            }
        }
        if let Some((t, v)) = last_tag(tags, &m.name) {
            if !versions.contains(&v) {
                out.push(Finding::new(
                    "reg.unpublished",
                    CheckSeverity::Error,
                    format!("`{t}` is not on crates-io as {}", m.name),
                ));
            }
        }
        if let (Some(w), Some(hp)) = (workspace::version_in_tree(root, m), versions.iter().max()) {
            if w < *hp {
                out.push(Finding::new(
                    "man.current.forward",
                    CheckSeverity::Error,
                    format!(
                        "`{}` working version {w} is below the published {hp}",
                        m.name
                    ),
                ));
            }
        }
    }
    out
}

/// The own lines that move, in name order: an own line that changed and did
/// not move refuses, and one that moves does so by one step.
pub fn moving(
    root: &Path,
    own: &[workspace::Member],
    head: &str,
    date: &str,
) -> Result<Vec<Line>, PlanError> {
    let mut out = Vec::new();
    for l in read(root, own, head, date)? {
        if !l.moves() {
            if !l.commits.is_empty() {
                return Err(PlanError::OwnUnmoved {
                    name: l.name,
                    tag:  l.last_tag.unwrap_or_default(),
                });
            }
            continue;
        }
        if let (Some(_), Some(from)) = (&l.last_tag, &l.from) {
            if !from.is_one_step(&l.to) {
                return Err(PlanError::NotOneStep {
                    what: format!("`{}`", l.name),
                    last: Box::new(from.clone()),
                    next: Box::new(l.to.clone()),
                });
            }
        }
        out.push(l);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
#[path = "lines_tests.rs"]
pub(crate) mod tests;
