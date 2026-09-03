//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The plan a release prints before it moves anything: the commits since the
//! last tag, the version the level makes, the changelog block, and what
//! publishes where, in dependency order.

use std::fmt;
use std::path::Path;

use homma_api::{Level, RepoKind, Version};

use super::check::{self, Packages};
use super::git::{self, GitError, Subject};
use super::registry::Registry;
use super::{changelog, kind, publish, version};

/// What one release will do, printed before anything moves and carried
/// through the steps that then do it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub repo_kind: RepoKind,
    /// The newest release tag, or none for a first release.
    pub last_tag:  Option<String>,
    pub current:   Version,
    pub next:      Version,
    pub tag:       String,
    pub commits:   Vec<Subject>,
    pub changelog: String,
    /// What publishes, in the order it publishes.
    pub publishes: Vec<(Registry, String)>,
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
        writeln!(
            f,
            "version {} becomes {}, tagged `{}`",
            self.current, self.next, self.tag
        )?;
        writeln!(f)?;
        f.write_str(&self.changelog)?;
        writeln!(f)?;
        if self.publishes.is_empty() {
            writeln!(f, "nothing publishes")?;
        } else {
            writeln!(f, "publishes, in order:")?;
            for (reg, name) in &self.publishes {
                writeln!(f, "  {name} to {reg}")?;
            }
        }
        Ok(())
    }
}

/// Why a plan could not be made: the tree, the manifest, the version, the
/// crate graph, or a manifest that disagrees with the level.
#[derive(Debug)]
pub enum PlanError {
    Git(GitError),
    NoManifest(kind::NoManifest),
    Version(version::VersionError),
    /// The publishable crates depend on each other in a cycle.
    Cycle(String),
    /// The manifest sits at a version that is neither the last tag's nor what
    /// the level makes of it. Boxed, since three versions inline would make
    /// every result carrying this error larger than the gate's clippy allows.
    OffLevel(Box<OffLevel>),
}

/// What a manifest off the level was measured against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffLevel {
    pub manifest: Version,
    pub last:     Version,
    pub level:    Level,
    pub next:     Version,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Git(e) => write!(f, "{e}"),
            PlanError::NoManifest(e) => write!(f, "{e}"),
            PlanError::Version(e) => write!(f, "{e}"),
            PlanError::Cycle(c) => write!(f, "dependency cycle among the crates: {c}"),
            PlanError::OffLevel(o) => {
                let OffLevel {
                    manifest,
                    last,
                    level,
                    next,
                } = &**o;
                // below the tag no level makes the manifest's version, so the
                // only offer is the tag's or the level's
                if manifest < last {
                    write!(
                        f,
                        "the manifest is at {manifest}, behind the last tag {last}; set it to \
                         {last} or to {next}, which a {level} release makes"
                    )
                } else {
                    write!(
                        f,
                        "the manifest is at {manifest} and a {level} release makes {next}; set it \
                         to {next} or pick the level that makes {manifest}"
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

/// The plan for releasing `head` at `level`, dated `date`.
pub fn plan(root: &Path, head: &str, level: Level, date: &str) -> Result<Plan, PlanError> {
    let repo_kind = kind::detect(root).map_err(PlanError::NoManifest)?;
    let current = version::read(root, repo_kind)?;
    let last = last_tag(root)?;
    let commits = match &last {
        Some((t, _)) => git::subjects(root, t, head)?,
        None => git::subjects_to(root, head)?,
    };
    // the level makes the version out of the last tag's; a manifest already
    // there is the release and nothing bumps, and a manifest anywhere else
    // above the tag disagrees with the level and is refused rather than
    // silently taken over it
    let next = match &last {
        Some((_, v)) => {
            let next = v.bumped(level);
            if current != *v && current != next {
                return Err(PlanError::OffLevel(Box::new(OffLevel {
                    manifest: current,
                    last: v.clone(),
                    level,
                    next,
                })));
            }
            next
        },
        None => current.bumped(level),
    };
    let tags = git::tags(root)?;
    let tag = check::tag_name(&tags, &next);
    let block = changelog::block(&next, date, &commits);
    let packages = check::packages(root, repo_kind);
    let publishes = publish_order(root, &packages).map_err(PlanError::Cycle)?;
    Ok(Plan {
        repo_kind,
        last_tag: last.map(|(t, _)| t),
        current,
        next,
        tag,
        commits,
        changelog: block,
        publishes,
    })
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
mod tests {
    use super::*;
    use crate::release::sh;

    fn repo(version: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| {
            let out = sh::run(d.path(), "git", args).unwrap();
            assert!(out.ok(), "{}", out.log());
        };
        g(&["init", "--quiet", "-b", "main"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        g(&["config", "tag.gpgsign", "false"]);
        g(&["config", "commit.gpgsign", "false"]);
        std::fs::write(
            d.path().join("Cargo.toml"),
            format!("[package]\nname = \"x\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
        g(&["add", "."]);
        g(&["commit", "--quiet", "-m", "feat: first"]);
        d
    }

    fn commit(d: &Path, subject: &str) {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::fs::write(d.join(format!("f{n}")), "x").unwrap();
        for args in [vec!["add", "."], vec!["commit", "--quiet", "-m", subject]] {
            assert!(sh::run(d, "git", &args).unwrap().ok());
        }
    }

    #[test]
    fn a_first_release_carries_the_whole_history_and_bumps_from_the_manifest() {
        let d = repo("0.1.0");
        commit(d.path(), "fix: two");
        let p = plan(d.path(), "HEAD", Level::Patch, "2026-09-02").unwrap();
        assert_eq!(p.last_tag, None);
        assert_eq!(p.commits.len(), 2);
        assert_eq!(p.commits[0].subject, "fix: two");
        assert_eq!(p.next, Version::new(0, 1, 1));
        assert_eq!(p.tag, "v0.1.1");
        assert!(p.changelog.starts_with("## 0.1.1 (2026-09-02)\n"));
        assert!(p.changelog.contains("### feat"));
        assert!(p.changelog.contains("### fix"));
        assert_eq!(p.publishes, vec![(Registry::CratesIo, "x".to_string())]);
        let shown = p.to_string();
        assert!(shown.contains("no release tag yet"));
        assert!(shown.contains("x to crates-io"));
    }

    #[test]
    fn commits_are_counted_from_the_newest_tag_and_the_tag_follows_the_convention() {
        let d = repo("0.1.0");
        assert!(
            sh::run(d.path(), "git", &["tag", "-a", "0.1.0", "-m", "0.1.0"])
                .unwrap()
                .ok()
        );
        commit(d.path(), "feat: later");
        let p = plan(d.path(), "HEAD", Level::Minor, "d").unwrap();
        assert_eq!(p.last_tag.as_deref(), Some("0.1.0"));
        assert_eq!(p.commits.len(), 1);
        assert_eq!(p.next, Version::new(0, 2, 0));
        assert_eq!(p.tag, "0.2.0", "bare tags stay bare");
        assert!(p.to_string().contains("since `0.1.0`"));
    }

    #[test]
    fn a_manifest_off_the_level_refuses_and_one_at_the_level_is_the_release() {
        let d = repo("0.1.0");
        assert!(
            sh::run(d.path(), "git", &["tag", "-a", "v0.1.0", "-m", "v0.1.0"])
                .unwrap()
                .ok()
        );
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
        )
        .unwrap();
        assert!(
            sh::run(d.path(), "git", &[
                "commit",
                "--quiet",
                "-am",
                "chore: bump"
            ])
            .unwrap()
            .ok()
        );
        // 0.1.1 is what a patch makes of v0.1.0, and not what a major makes
        let err = plan(d.path(), "HEAD", Level::Major, "d").unwrap_err();
        assert!(
            matches!(&err, PlanError::OffLevel(o)
                if o.manifest == Version::new(0, 1, 1) && o.next == Version::new(0, 2, 0)),
            "{err}"
        );
        assert!(err.to_string().contains("0.1.1"));
        assert!(err.to_string().contains("0.2.0"));
        assert!(
            err.to_string().contains("pick the level"),
            "above the tag a level may make the manifest's version: {err}"
        );
        // below the tag no level makes it, and the message does not offer one
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.0.9\"\n",
        )
        .unwrap();
        commit(d.path(), "chore: back too far");
        let err = plan(d.path(), "HEAD", Level::Patch, "d").unwrap_err();
        assert!(matches!(&err, PlanError::OffLevel(_)), "{err}");
        assert!(
            err.to_string().contains("behind the last tag 0.1.0"),
            "{err}"
        );
        assert!(!err.to_string().contains("pick the level"), "{err}");
        // and back at what a patch makes of the tag
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.1\"\n",
        )
        .unwrap();
        commit(d.path(), "chore: at the level");
        let p = plan(d.path(), "HEAD", Level::Patch, "d").unwrap();
        assert_eq!(
            p.next,
            Version::new(0, 1, 1),
            "already at the level's version, so that is the release"
        );
        std::fs::write(
            d.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert!(
            sh::run(d.path(), "git", &[
                "commit",
                "--quiet",
                "-am",
                "chore: back"
            ])
            .unwrap()
            .ok()
        );
        let p = plan(d.path(), "HEAD", Level::Major, "d").unwrap();
        assert_eq!(p.next, Version::new(0, 2, 0), "a major on 0.x is a minor");
    }

    #[test]
    fn a_tree_without_a_manifest_is_not_plannable() {
        let d = tempfile::tempdir().unwrap();
        assert!(sh::run(d.path(), "git", &["init", "--quiet"]).unwrap().ok());
        assert!(matches!(
            plan(d.path(), "HEAD", Level::Patch, "d"),
            Err(PlanError::NoManifest(_))
        ));
    }
}
