//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The plan a release prints before it moves anything: the commits since the
//! last tag, the version the level or the named version makes, the changelog
//! block, and what publishes where, in dependency order. A virtual workspace
//! plans its primary's version and names each rider as staying or moving, and
//! a member on a line of its own is planned against its own tags.

use std::fmt;
use std::path::Path;

use homma_api::{Level, Markers, RepoKind, Target, Version};

use super::check::{self, Packages};
use super::git::{self, GitError, Subject};
use super::lines::{self, Line};
use super::registry::Registry;
use super::workspace::{self, Layout};
use super::{changelog, kind, publish, version};

/// What a run is asked to release: the repo's line at a target, every own
/// line that moves riding it, or the own lines alone, the repo's staying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aim {
    Repo(Target),
    OwnLines,
}

impl From<Target> for Aim {
    fn from(t: Target) -> Self {
        Aim::Repo(t)
    }
}

impl From<Level> for Aim {
    fn from(l: Level) -> Self {
        Aim::Repo(l.into())
    }
}

impl From<Version> for Aim {
    fn from(v: Version) -> Self {
        Aim::Repo(v.into())
    }
}

impl From<Option<Target>> for Aim {
    fn from(t: Option<Target>) -> Self {
        t.map_or(Aim::OwnLines, Aim::Repo)
    }
}

pub use super::plan_error::{OffLevel, PlanError};

/// A member of a virtual workspace that is not its primary, with the version
/// its manifest carried at the last tag and carries now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ride {
    pub name:    String,
    pub from:    Option<Version>,
    pub to:      Version,
    /// Whether anything under its directory changed since the last tag.
    pub changed: bool,
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
    pub repo_kind:  RepoKind,
    pub layout:     Layout,
    /// The newest release tag, or none for a first release.
    pub last_tag:   Option<String>,
    pub current:    Version,
    pub next:       Version,
    pub tag:        String,
    /// Whether the repo's own line releases; where it does not, `next` is
    /// `current` and only the own lines release.
    pub repo_moves: bool,
    pub commits:    Vec<Subject>,
    pub changelog:  String,
    /// The riders of a virtual workspace; none for a single version.
    pub riders:     Vec<Ride>,
    /// The members on lines of their own that move, in name order.
    pub lines:      Vec<Line>,
    /// What publishes, in the order it publishes, at the version it publishes.
    pub publishes:  Vec<(Registry, String, Version)>,
}

impl Plan {
    /// What the bump and the merge are named by: the repo's next version where
    /// its line moves, else each moving own line as `<crate> <version>`.
    pub fn name(&self) -> String {
        if self.repo_moves {
            return self.next.to_string();
        }
        self.lines
            .iter()
            .map(|l| format!("{} {}", l.name, l.to))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Every tag the release makes, the repo's first.
    pub fn tags(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .repo_moves
            .then(|| self.tag.clone())
            .into_iter()
            .collect();
        out.extend(self.lines.iter().map(|l| l.tag.clone()));
        out
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.repo_moves {
            writeln!(
                f,
                "the repo's own line has nothing since `{}` and stays at {}",
                self.last_tag.as_deref().unwrap_or_default(),
                self.current
            )?;
        } else {
            self.fmt_repo(f)?;
        }
        for l in &self.lines {
            match &l.from {
                Some(from) if l.last_tag.is_some() => {
                    writeln!(
                        f,
                        "`{}` moves from {from} to {}, tagged `{}`",
                        l.name, l.to, l.tag
                    )?
                },
                _ => {
                    writeln!(
                        f,
                        "`{}` is new on its own line at {}, tagged `{}`",
                        l.name, l.to, l.tag
                    )?
                },
            }
        }
        if self.repo_moves {
            writeln!(f)?;
            f.write_str(&self.changelog)?;
        }
        for l in &self.lines {
            writeln!(f)?;
            writeln!(f, "into {}/CHANGELOG.md:", l.dir())?;
            f.write_str(&l.changelog)?;
        }
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

impl Plan {
    fn fmt_repo(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
                (Some(_), false) if r.changed => {
                    writeln!(
                        f,
                        "warning: `{}` changed since `{}` and stays at {}; its change publishes \
                         only once its version moves",
                        r.name,
                        self.last_tag.as_deref().unwrap_or_default(),
                        r.to
                    )?
                },
                (Some(_), false) => writeln!(f, "`{}` stays at {}", r.name, r.to)?,
            }
        }
        Ok(())
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

/// The plan for releasing `head` as `aim` asks, dated `date`.
pub fn plan(
    root: &Path,
    repo: &str,
    markers: &Markers,
    head: &str,
    aim: impl Into<Aim>,
    date: &str,
) -> Result<Plan, PlanError> {
    let aim = aim.into();
    let repo_kind = kind::detect(root, markers).map_err(PlanError::NoManifest)?;
    let own = if repo_kind.has_crate() { lines::own(root) } else { Vec::new() };
    let layout = workspace::layout(root, repo_kind, repo).map_err(PlanError::Layout)?;
    let (layout, own_primary) = lines::strip(layout, &own);
    if let Some(name) = own_primary {
        return Err(PlanError::OwnPrimary(name));
    }
    let current = working_version(root, repo_kind, &layout)?;
    let last = last_tag(root)?;
    // the repo's own line is what lies outside the own lines; with none, every
    // commit, read as it always was
    let from = last.as_ref().map(|(t, _)| t.as_str());
    let commits = if own.is_empty() {
        match from {
            Some(f) => git::subjects(root, f, head)?,
            None => git::subjects_to(root, head)?,
        }
    } else {
        lines::repo_commits(root, &own, from, head)?
    };
    let lines = lines::moving(root, &own, head, date)?;
    let target = match aim {
        Aim::Repo(_) if commits.is_empty() && !own.is_empty() => return Err(PlanError::Nothing),
        Aim::Repo(t) => Some(t),
        Aim::OwnLines if !commits.is_empty() => {
            return Err(PlanError::RepoUnreleased(commits.len()));
        },
        Aim::OwnLines if lines.is_empty() => return Err(PlanError::Nothing),
        Aim::OwnLines => None,
    };
    let tags = git::tags(root)?;
    let Some(target) = target else {
        let publishes = publishes_of(root, repo_kind, &layout, &[], &lines, None)?;
        return Ok(Plan {
            repo_kind,
            layout,
            last_tag: last.map(|(t, _)| t),
            tag: check::tag_name(&tags, &current),
            next: current.clone(),
            current,
            repo_moves: false,
            commits,
            changelog: String::new(),
            riders: Vec::new(),
            lines,
            publishes,
        });
    };
    let next = next_version(&current, last.as_ref().map(|(_, v)| v), &target)?;
    let tag = check::tag_name(&tags, &next);
    let riders = rides(root, &layout, last.as_ref().map(|(t, _)| t.as_str()), head)?;
    // a rider moves one step, as the primary does
    for r in riders.iter().filter(|r| r.moves()) {
        if let Some(from) = &r.from {
            if !from.is_one_step(&r.to) {
                return Err(PlanError::NotOneStep {
                    what: format!("`{}`", r.name),
                    last: Box::new(from.clone()),
                    next: Box::new(r.to.clone()),
                });
            }
        }
    }
    let mut block = changelog::block(&next, date, &commits);
    let moved: Vec<(String, Version)> = riders
        .iter()
        .filter(|r| r.moves())
        .map(|r| (r.name.clone(), r.to.clone()))
        .collect();
    changelog::name_the_moves(&mut block, &moved);
    let publishes = publishes_of(root, repo_kind, &layout, &moved, &lines, Some(&next))?;
    Ok(Plan {
        repo_kind,
        layout,
        last_tag: last.map(|(t, _)| t),
        current,
        next,
        tag,
        repo_moves: true,
        commits,
        changelog: block,
        riders,
        lines,
        publishes,
    })
}

/// What publishes and at which version: the repo's crates at `next` where its
/// line moves, a set's primary and its moved riders, and every moving own
/// line at its own version; in dependency order.
fn publishes_of(
    root: &Path,
    repo_kind: RepoKind,
    layout: &Layout,
    moved: &[(String, Version)],
    lines: &[Line],
    next: Option<&Version>,
) -> Result<Vec<(Registry, String, Version)>, PlanError> {
    let mut packages = check::packages(root, repo_kind);
    let on_line = |c: &String| lines.iter().any(|l| l.name == *c);
    let own = lines::own(root);
    let is_own = |c: &String| own.iter().any(|m| m.name == *c);
    match (next, layout) {
        (None, _) => {
            packages.crates.retain(on_line);
            packages.jsr = None;
            packages.npm = None;
        },
        (
            Some(_),
            Layout::Set {
                primary,
                ..
            },
        ) => {
            packages
                .crates
                .retain(|c| *c == primary.name || moved.iter().any(|(m, _)| m == c) || on_line(c));
        },
        (Some(_), Layout::Single) => packages.crates.retain(|c| !is_own(c) || on_line(c)),
    }
    Ok(publish_order(root, &packages)
        .map_err(PlanError::Cycle)?
        .into_iter()
        .map(|(reg, name)| {
            let v = moved
                .iter()
                .find(|(m, _)| *m == name)
                .map(|(_, v)| v.clone())
                .or_else(|| lines.iter().find(|l| l.name == name).map(|l| l.to.clone()))
                .or_else(|| next.cloned())
                .unwrap_or_else(|| Version::new(0, 0, 0));
            (reg, name, v)
        })
        .collect())
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
    // a first release named outright is the manifest's version or one step
    // above it
    let Some(v) = last else {
        return match target {
            Target::Level(l) => Ok(current.bumped(*l)),
            Target::Version(n) if n == current || current.is_one_step(n) => Ok(n.clone()),
            Target::Version(n) => {
                Err(PlanError::NotOneStep {
                    what: "the manifest".into(),
                    last: Box::new(current.clone()),
                    next: Box::new(n.clone()),
                })
            },
        };
    };
    let next = target.next_from(v);
    if !v.is_one_step(&next) {
        return Err(PlanError::NotOneStep {
            what: "the last tag".into(),
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

/// Each rider's version at the last tag and at `head`, and whether its
/// directory changed between the two.
fn rides(
    root: &Path,
    layout: &Layout,
    last_tag: Option<&str>,
    head: &str,
) -> Result<Vec<Ride>, GitError> {
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
        let (from, changed) = match last_tag {
            Some(t) => {
                let dir = m.manifest.rsplit_once('/').map_or(".", |(d, _)| d);
                (
                    workspace::version_at(root, t, &m.manifest)?,
                    git::changed_between(root, t, head, dir)?,
                )
            },
            None => (None, false),
        };
        out.push(Ride {
            name: m.name.clone(),
            from,
            to,
            changed,
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
