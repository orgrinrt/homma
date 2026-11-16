//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Where the forge takes a branch only through a pull request: the pushes of
//! steps 4 and 5 landed that way instead, and the bump a run that stopped
//! there left, on the trunk or already on the forge, set aside and taken
//! back when it runs again.

use std::path::Path;

use homma_api::Version;

use super::git::{self, GitError};
use super::run::{Left, ReleaseError, Setup};
use super::writes::Merge;

/// Whether a failed push is the forge refusing it outright, a ruleset, in
/// git's words for it. The workspace's landing scripts read the same words.
pub fn refused(e: &GitError) -> bool {
    matches!(e, GitError::Failed { stderr, .. } if stderr.contains("[remote rejected]"))
}

/// What a bump commit's subject opens with, the version after it.
const BUMP: &str = "chore: release ";

/// The subject of the bump commit for `version`.
pub fn bump_subject(version: &Version) -> String {
    format!("{BUMP}{version}")
}

/// The branch a bump is pushed as when the trunk takes no push.
pub fn bump_branch(repo: &str, version: &Version) -> String {
    format!("release/{repo}-{version}")
}

/// Step 4's push of the bump commit `sha`: the trunk pushed, or where the
/// forge refuses it, `sha` pushed as its own branch and squashed into the
/// trunk through a pull request. The clone's trunk then takes the forge's
/// commit, checked to carry the bump's tree on the bump's parent, the
/// branch is dropped through the forge, and that commit is what is returned.
pub fn push_bump(
    setup: &Setup<'_>,
    root: &Path,
    sha: &str,
    version: &Version,
) -> Result<String, ReleaseError> {
    match git::push(root, setup.remote, setup.trunk, false) {
        Ok(()) => return Ok(sha.to_string()),
        Err(e) if refused(&e) => {},
        Err(e) => return Err(e.into()),
    }
    let head = bump_branch(setup.repo, version);
    git::push(
        root,
        setup.remote,
        &format!("{sha}:refs/heads/{head}"),
        false,
    )?;
    setup.forge.land(
        root,
        &head,
        setup.trunk,
        &bump_subject(version),
        Merge::Squash,
        sha,
    )?;
    let landed = git::fetch_branch(root, setup.remote, setup.trunk)?;
    if git::tree(root, &landed)? != git::tree(root, sha)?
        || git::sha(root, &format!("{landed}^"))? != git::sha(root, &format!("{sha}^"))?
    {
        return Err(ReleaseError::NotLanded {
            base: setup.trunk.to_string(),
            at:   landed,
            left: Left::Bump(sha.to_string()),
        });
    }
    git::reset_hard(root, &landed)?;
    setup.forge.drop_branch(root, &head, sha)?;
    Ok(landed)
}

/// Step 5 where the release line refused the push of the local merge, which
/// the caller has undone: the release pull request, the trunk into the
/// release line, merged by the forge with a merge commit at the trunk's tip.
/// The clone's release line takes that commit once it is checked to be a
/// merge of the tip onto where the line was, and it is returned for the tag.
pub fn land_release(
    setup: &Setup<'_>,
    root: &Path,
    version: &Version,
) -> Result<String, ReleaseError> {
    let tip = git::sha(root, setup.trunk)?;
    let before = git::sha(root, setup.release)?;
    setup.forge.land(
        root,
        setup.trunk,
        setup.release,
        &format!("release: {version}"),
        Merge::Merge,
        &tip,
    )?;
    let landed = git::fetch_branch(root, setup.remote, setup.release)?;
    if git::parent_count(root, &landed)? != 2
        || git::sha(root, &format!("{landed}^2"))? != tip
        || git::sha(root, &format!("{landed}^1"))? != before
    {
        return Err(ReleaseError::NotLanded {
            base: setup.release.to_string(),
            at:   landed,
            left: Left::Untagged(tip),
        });
    }
    git::set_branch(root, setup.release, &landed)?;
    Ok(landed)
}

/// Where a bump a stopped run left stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landed {
    /// The clone alone holds it, and step 4 pushes it.
    Local,
    /// The remote's trunk holds it, and step 4 takes it as it is.
    Trunk,
    /// The remote's release line holds this merge of it too, and step 5
    /// takes the merge and tags it.
    Release(String),
}

/// A bump commit a run that stopped after making it left, on the trunk or on
/// the forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The bump as the trunk is to hold it.
    pub sha:      String,
    /// The version its subject names.
    pub version:  Version,
    pub landed:   Landed,
    /// The clone's own bump where the forge squashed it into `sha`: the head
    /// `release/<repo>-<version>` was pushed at, still to be dropped.
    pub squashed: Option<String>,
}

/// The version a bump commit carries: one parent, and the subject
/// `chore: release <version>` with a version that reads as one.
fn bump_version(root: &Path, rev: &str) -> Result<Option<Version>, ReleaseError> {
    if git::parent_count(root, rev)? != 1 {
        return Ok(None);
    }
    let subject = git::subject(root, rev)?;
    Ok(subject
        .strip_prefix(BUMP)
        .and_then(|v| v.parse::<Version>().ok()))
}

/// Before step 1: the trunk and the release line fetched, and where the
/// checked-out, clean trunk ends in a bump nothing tagged yet, it is set
/// aside, the trunk moved back to its parent, so the check, the gate run and
/// the plan see the commit the gate measured. The bump is the clone's own,
/// one past the remote's tip; or the remote's tip itself, a pull request that
/// merged unseen, the clone's tip or a commit with the clone's tree on the
/// clone's parent; and where the release line's first parents hold a merge of
/// it, that merge too. Anything else is no pending bump and moves nothing.
pub fn set_aside(setup: &Setup<'_>, root: &Path) -> Result<Option<Pending>, ReleaseError> {
    if git::current_branch(root)?.as_deref() != Some(setup.trunk) || !git::is_clean(root)? {
        return Ok(None);
    }
    let Ok(remote) = git::fetch_branch(root, setup.remote, setup.trunk) else {
        return Ok(None);
    };
    let tip = git::head(root)?;
    let Some(version) = bump_version(root, &tip)? else {
        return Ok(None);
    };
    let parent = git::sha(root, &format!("{tip}^"))?;
    if parent == remote {
        git::reset_hard(root, &remote)?;
        return Ok(Some(Pending {
            sha: tip,
            version,
            landed: Landed::Local,
            squashed: None,
        }));
    }
    let (sha, squashed) = if tip == remote {
        (tip, None)
    } else if bump_version(root, &remote)?.as_ref() == Some(&version)
        && git::sha(root, &format!("{remote}^"))? == parent
        && git::tree(root, &remote)? == git::tree(root, &tip)?
    {
        (remote, Some(tip))
    } else {
        return Ok(None);
    };
    if git::tagged(root, &sha)? {
        return Ok(None);
    }
    let landed = match release_merge(setup, root, &sha)? {
        Some(merge) => Landed::Release(merge),
        None => Landed::Trunk,
    };
    git::reset_hard(root, &parent)?;
    Ok(Some(Pending {
        sha,
        version,
        landed,
        squashed,
    }))
}

/// The merge of `bump` on the remote's release line, fetched, where its first
/// parents hold one.
fn release_merge(
    setup: &Setup<'_>,
    root: &Path,
    bump: &str,
) -> Result<Option<String>, ReleaseError> {
    if setup.trunk == setup.release {
        return Ok(None);
    }
    let Ok(line) = git::fetch_branch(root, setup.remote, setup.release) else {
        return Ok(None);
    };
    Ok(git::merge_of(root, &line, bump)?)
}

/// The set-aside bump back on the trunk, after a step before 4 refused.
pub fn put_back(root: &Path, pending: Option<&Pending>) -> Result<(), ReleaseError> {
    let Some(p) = pending else {
        return Ok(());
    };
    git::reset_hard(root, &p.sha).map_err(|e| {
        ReleaseError::NotPutBack {
            sha: p.sha.clone(),
            why: e.to_string(),
        }
    })
}

/// Step 4 with a pending bump: refused, the bump put back, where it is
/// another version than the plan's; else taken back, and pushed where only
/// the clone held it, or its squashed branch dropped where the forge
/// squashed it unseen.
pub fn resume_bump(
    setup: &Setup<'_>,
    root: &Path,
    pending: &Pending,
    next: &Version,
) -> Result<String, ReleaseError> {
    if pending.version != *next {
        put_back(root, Some(pending))?;
        return Err(ReleaseError::PendingBump {
            found:   bump_subject(&pending.version),
            planned: next.to_string(),
        });
    }
    git::fast_forward(root, &pending.sha)?;
    match (&pending.landed, &pending.squashed) {
        (Landed::Local, _) => push_bump(setup, root, &pending.sha, next),
        (_, Some(head)) => {
            setup
                .forge
                .drop_branch(root, &bump_branch(setup.repo, next), head)?;
            Ok(pending.sha.clone())
        },
        (_, None) => Ok(pending.sha.clone()),
    }
}
