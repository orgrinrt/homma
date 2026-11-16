//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The plan a release prints before it moves anything: the commits since the
//! last tag, the version the level or the named version makes, the changelog
//! block, and what publishes where, in dependency order. A virtual workspace
//! plans its primary's version and names each rider as staying or moving.

use std::fmt;
use std::path::Path;

use homma_api::{Markers, RepoKind, Target, Version};

use super::check::{self, Packages};
use super::git::{self, GitError, Subject};
use super::registry::Registry;
use super::workspace::{self, Layout, NoPrimary};
use super::{changelog, kind, publish, version};

/// A member of a virtual workspace that is not its primary, with the version
/// its manifest carried at the last tag and carries now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ride {
    pub name: String,
    pub from: Option<Version>,
    pub to:   Version,
}

impl Ride {
    /// Whether its manifest moved since the last tag, so it publishes.
    pub fn moves(&self) -> bool {
        self.from.as_ref() != Some(&self.to)
    }
}

/// What one release will do, printed before anything moves and carried
/// through the steps that then do it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub repo_kind: RepoKind,
    pub layout:    Layout,
    /// The newest release tag, or none for a first release.
    pub last_tag:  Option<String>,
    pub current:   Version,
    pub next:      Version,
    pub tag:       String,
    pub commits:   Vec<Subject>,
    pub changelog: String,
    /// The riders of a virtual workspace; none for a single version.
    pub riders:    Vec<Ride>,
    /// What publishes, in the order it publishes, at the version it publishes.
    pub publishes: Vec<(Registry, String, Version)>,
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.last_tag {
            Some(t) => writeln!(f, "{} commit(s) since `{t}`:", self.commits.len())?,
            None => {
                writeln!(
                    f,
                    "{} commit(s), and no release tag yet:",
                    self.commits.len()
                )?
            },
        }
        for c in &self.commits {
            writeln!(f, "  {} {}", c.sha, c.subject)?;
        }
        match &self.layout {
            Layout::Set {
                primary,
                ..
            } => {
                writeln!(
                    f,
                    "`{}` {} becomes {}, tagged `{}`",
                    primary.name, self.current, self.next, self.tag
                )?
            },
            Layout::Single => {
                writeln!(
                    f,
                    "version {} becomes {}, tagged `{}`",
                    self.current, self.next, self.tag
                )?
            },
        }
        for r in &self.riders {
            match (&r.from, r.moves()) {
                (Some(from), true) => writeln!(f, "`{}` moves from {from} to {}", r.name, r.to)?,
                (None, _) => writeln!(f, "`{}` is new at {}", r.name, r.to)?,
                (Some(_), false) => writeln!(f, "`{}` stays at {}", r.name, r.to)?,
            }
        }
        writeln!(f)?;
        f.write_str(&self.changelog)?;
        writeln!(f)?;
        if self.publishes.is_empty() {
            writeln!(f, "nothing publishes")?;
        } else {
            writeln!(f, "publishes, in order:")?;
            for (reg, name, v) in &self.publishes {
                writeln!(f, "  {name} {v} to {reg}")?;
            }
        }
        Ok(())
    }
}

/// Why a plan could not be made: the tree, the manifest, the version, the
/// crate graph, a manifest that disagrees with the target, a named version no
/// single step above the last tag, or a virtual workspace with no primary.
#[derive(Debug)]
pub enum PlanError {
    Git(GitError),
    NoManifest(kind::NoManifest),
    Version(version::VersionError),
    /// The publishable crates depend on each other in a cycle.
    Cycle(String),
    /// The manifest sits at a version that is neither the last tag's nor what
    /// the target makes of it. Boxed, since three versions inline would make
    /// every result carrying this error larger than the gate's clippy allows.
    OffLevel(Box<OffLevel>),
    /// The version named is not one step above the last tag's.
    NotOneStep {
        last: Box<Version>,
        next: Box<Version>,
    },
    Layout(NoPrimary),
}

/// What a manifest off the target was measured against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffLevel {
    pub manifest: Version,
    pub last:     Version,
    pub target:   Target,
    pub next:     Version,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Git(e) => write!(f, "{e}"),
            PlanError::NoManifest(e) => write!(f, "{e}"),
            PlanError::Version(e) => write!(f, "{e}"),
            PlanError::Cycle(c) => write!(f, "dependency cycle among the crates: {c}"),
            PlanError::Layout(e) => write!(f, "{e}"),
            PlanError::NotOneStep {
                last,
                next,
            } => {
                write!(
                    f,
                    "{next} is not one step above the last tag {last}: a level's step, a \
                     pre-release of one, or from a pre-release a later one or its release"
                )
            },
            PlanError::OffLevel(o) => {
                let OffLevel {
                    manifest,
                    last,
                    target,
                    next,
                } = &**o;
                // below the tag no level makes the manifest's version, so the
                // only offer is the tag's or the target's
                if manifest < last {
                    write!(
                        f,
                        "the manifest is at {manifest}, behind the last tag {last}; set it to \
                         {last} or to {next}, which a {target} makes"
                    )
                } else {
                    write!(
                        f,
                        "the manifest is at {manifest} and a {target} makes {next}; set it to \
                         {next} or pick the level that makes {manifest}, or name it with \
                         `--version`"
                    )
                }
            },
        }
    }
}

impl std::error::Error for PlanError {}

impl From<GitError> for PlanError {
    fn from(e: GitError) -> Self {
        PlanError::Git(e)
    }
}

impl From<version::VersionError> for PlanError {
    fn from(e: version::VersionError) -> Self {
        PlanError::Version(e)
    }
}

/// The newest release tag by version, with its version.
pub fn last_tag(root: &Path) -> Result<Option<(String, Version)>, GitError> {
    let mut versioned: Vec<(String, Version)> = git::tags(root)?
        .into_iter()
        .filter_map(|t| check::tag_version(&t).map(|v| (t, v)))
        .collect();
    versioned.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(versioned.pop())
}

/// The version the release is about: the root's, or a set's primary's.
pub fn working_version(
    root: &Path,
    repo_kind: RepoKind,
    layout: &Layout,
) -> Result<Version, version::VersionError> {
    match layout {
        Layout::Single => version::read(root, repo_kind),
        Layout::Set {
            primary,
            ..
        } => {
            workspace::version_in_tree(root, primary).ok_or_else(|| {
                version::VersionError::Manifest(primary.manifest.clone(), "no version".into())
            })
        },
    }
}

/// The plan for releasing `head` to `target`, dated `date`.
pub fn plan(
    root: &Path,
    markers: &Markers,
    head: &str,
    target: impl Into<Target>,
    date: &str,
) -> Result<Plan, PlanError> {
    let target = target.into();
    let repo_kind = kind::detect(root, markers).map_err(PlanError::NoManifest)?;
    let layout = workspace::layout(root, repo_kind).map_err(PlanError::Layout)?;
    let current = working_version(root, repo_kind, &layout)?;
    let last = last_tag(root)?;
    let commits = match &last {
        Some((t, _)) => git::subjects(root, t, head)?,
        None => git::subjects_to(root, head)?,
    };
    let next = next_version(&current, last.as_ref().map(|(_, v)| v), &target)?;
    let tags = git::tags(root)?;
    let tag = check::tag_name(&tags, &next);
    let riders = rides(root, &layout, last.as_ref().map(|(t, _)| t.as_str()))?;
    let mut block = changelog::block(&next, date, &commits);
    let moved: Vec<(String, Version)> = riders
        .iter()
        .filter(|r| r.moves())
        .map(|r| (r.name.clone(), r.to.clone()))
        .collect();
    changelog::name_the_moves(&mut block, &moved);
    let mut packages = check::packages(root, repo_kind);
    if let Layout::Set {
        primary,
        ..
    } = &layout
    {
        packages
            .crates
            .retain(|c| *c == primary.name || moved.iter().any(|(m, _)| m == c));
    }
    let publishes = publish_order(root, &packages)
        .map_err(PlanError::Cycle)?
        .into_iter()
        .map(|(reg, name)| {
            let v = moved
                .iter()
                .find(|(m, _)| *m == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| next.clone());
            (reg, name, v)
        })
        .collect();
    Ok(Plan {
        repo_kind,
        layout,
        last_tag: last.map(|(t, _)| t),
        current,
        next,
        tag,
        commits,
        changelog: block,
        riders,
        publishes,
    })
}

/// The target makes the version out of the last tag's; a manifest already
/// there is the release and nothing bumps, and a manifest anywhere else above
/// the tag disagrees with the target and is refused rather than silently
/// taken over it. A version named outright must be one step above the tag.
fn next_version(
    current: &Version,
    last: Option<&Version>,
    target: &Target,
) -> Result<Version, PlanError> {
    let Some(v) = last else {
        return Ok(match target {
            Target::Level(l) => current.bumped(*l),
            Target::Version(n) => n.clone(),
        });
    };
    let next = target.next_from(v);
    if !v.is_one_step(&next) {
        return Err(PlanError::NotOneStep {
            last: Box::new(v.clone()),
            next: Box::new(next),
        });
    }
    if current != v && *current != next {
        return Err(PlanError::OffLevel(Box::new(OffLevel {
            manifest: current.clone(),
            last: v.clone(),
            target: target.clone(),
            next,
        })));
    }
    Ok(next)
}

/// Each rider's version at the last tag and in the tree.
fn rides(root: &Path, layout: &Layout, last_tag: Option<&str>) -> Result<Vec<Ride>, GitError> {
    let Layout::Set {
        riders,
        ..
    } = layout
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for m in riders {
        let Some(to) = workspace::version_in_tree(root, m) else {
            continue;
        };
        let from = match last_tag {
            Some(t) => workspace::version_at(root, t, &m.manifest)?,
            None => None,
        };
        out.push(Ride {
            name: m.name.clone(),
            from,
            to,
        });
    }
    Ok(out)
}

/// Crates in dependency order, then jsr, then npm, since the npm build is
/// made off the same source jsr took.
pub fn publish_order(root: &Path, packages: &Packages) -> Result<Vec<(Registry, String)>, String> {
    let mut out: Vec<(Registry, String)> = publish::crate_order(root, &packages.crates)?
        .into_iter()
        .map(|(name, _)| (Registry::CratesIo, name))
        .collect();
    if let Some(j) = &packages.jsr {
        out.push((Registry::Jsr, j.clone()));
    }
    if let Some(n) = &packages.npm {
        out.push((Registry::Npm, n.clone()));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
