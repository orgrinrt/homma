//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Why a plan could not be made, each refusal in the words the plan says it.

use std::fmt;

use homma_api::{Target, Version};

use super::git::GitError;
use super::workspace::NoPrimary;
use super::{kind, lines, version};

/// Why a plan could not be made: the tree, the manifest, the version, the
/// crate graph, a manifest that disagrees with the target, a named version no
/// single step above the last tag, a virtual workspace with no primary, or
/// lines of their own that leave nothing or too much to release.
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
    /// A version that is not one step above the one before it: the release's
    /// next against the last tag or the manifest, or a rider's or an own
    /// line's move.
    NotOneStep {
        what: String,
        last: Box<Version>,
        next: Box<Version>,
    },
    Layout(NoPrimary),
    /// A virtual workspace's primary declared on a line of its own.
    OwnPrimary(String),
    /// A member on a line of its own changed since its last own tag, or has
    /// none, and its version did not move.
    OwnUnmoved {
        name: String,
        tag:  String,
    },
    /// No target named, and the repo's own line has commits since its last
    /// tag.
    RepoUnreleased(usize),
    /// Nothing on the repo's line and no own line moving.
    Nothing,
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
                what,
                last,
                next,
            } => {
                write!(
                    f,
                    "{next} is not one step above {what} at {last}: a level's step, a \
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
            PlanError::OwnPrimary(name) => {
                write!(
                    f,
                    "`{name}` is the virtual workspace's primary, whose line is the repo's, and \
                     its `release.toml` puts it on a line of its own"
                )
            },
            PlanError::OwnUnmoved {
                name,
                tag,
            } => {
                write!(
                    f,
                    "`{name}` changed since `{tag}` and its version did not move, so `main` \
                     would carry it unreleased; move its version on `dev`"
                )
            },
            PlanError::RepoUnreleased(n) => {
                write!(
                    f,
                    "the repo's own line has {n} commit(s) since its last tag; name a `--level` \
                     or a `--version` for it"
                )
            },
            PlanError::Nothing => {
                write!(
                    f,
                    "nothing to release: the repo's own line has no commit since its last tag \
                     and no member on a line of its own moves"
                )
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

impl From<lines::ReadError> for PlanError {
    fn from(e: lines::ReadError) -> Self {
        match e {
            lines::ReadError::Git(e) => PlanError::Git(e),
            lines::ReadError::NoVersion(name) => {
                PlanError::Version(version::VersionError::Manifest(name, "no version".into()))
            },
        }
    }
}
