//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The check of a virtual workspace's riders, each against its own versions.
//! A rider has no tag of its own: its versions are those a release tag's
//! commit carries, each bump of it on the release line lands on one, and its
//! version at the newest tag is the one that release published.

use std::path::Path;

use homma_api::{CheckSeverity, Finding, Version};

use super::check::Published;
use super::git::{self, GitError};
use super::registry::Registry;
use super::workspace::{self, Member};

/// Every finding about `riders`, given the release tags oldest first and
/// whether the release line exists to walk.
pub fn findings(
    root: &Path,
    riders: &[Member],
    tags: &[(String, Version)],
    release: Option<&str>,
    published: &Published,
) -> Result<Vec<Finding>, GitError> {
    let mut out = Vec::new();
    for r in riders {
        let mut tagged = Vec::new();
        for (t, _) in tags {
            if let Some(v) = workspace::version_at(root, t, &r.manifest)? {
                tagged.push(v);
            }
        }
        if let Some(release) = release {
            for sha in git::first_parent_touching(root, release, &r.manifest)? {
                let here = workspace::version_at(root, &sha, &r.manifest)?;
                let parent = workspace::version_at(root, &format!("{sha}^"), &r.manifest)?;
                if let (Some(here), Some(parent)) = (here, parent) {
                    if here != parent && !tagged.contains(&here) {
                        out.push(Finding::new(
                            "tag.bump.tagged",
                            CheckSeverity::Error,
                            format!(
                                "{} bumps `{}` to {here} and no release tag carries it",
                                &sha[.. 7],
                                r.name
                            ),
                        ));
                    }
                }
            }
        }
        let versions = published
            .get(Registry::CratesIo, &r.name)
            .cloned()
            .unwrap_or_default();
        if let (Some(w), Some(hp)) = (workspace::version_in_tree(root, r), versions.iter().max()) {
            if w < *hp {
                out.push(Finding::new(
                    "man.current.forward",
                    CheckSeverity::Error,
                    format!(
                        "`{}` working version {w} is below the published {hp}",
                        r.name
                    ),
                ));
            }
        }
        for v in &versions {
            if !tagged.contains(v) {
                out.push(Finding::new(
                    "reg.orphan",
                    CheckSeverity::Error,
                    format!(
                        "{} {v} is on crates-io and no release tag carries it",
                        r.name
                    ),
                ));
            }
        }
        if let Some((t, _)) = tags.last() {
            if let Some(v) = workspace::version_at(root, t, &r.manifest)? {
                if !versions.contains(&v) {
                    out.push(Finding::new(
                        "reg.unpublished",
                        CheckSeverity::Error,
                        format!("`{t}` carries {} {v}, which is not on crates-io", r.name),
                    ));
                }
            }
        }
    }
    Ok(out)
}
