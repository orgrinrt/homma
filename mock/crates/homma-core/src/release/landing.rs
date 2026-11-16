//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Where the forge takes a branch only through a pull request: the pushes of
//! steps 4 and 5 landed that way instead, and the bump a run that stopped
//! there left on the trunk, set aside and taken back when it runs again.

use std::path::Path;

use homma_api::Version;

use super::git::{self, GitError};
use super::run::{ReleaseError, Setup};
use super::writes::Merge;

/// Whether a failed push is the forge refusing it outright, a ruleset, in
/// git's words for it. The workspace's landing scripts read the same words.
pub fn refused(e: &GitError) -> bool {
    matches!(e, GitError::Failed { stderr, .. } if stderr.contains("[remote rejected]"))
}

/// The subject of the bump commit for `version`.
pub fn bump_subject(version: &Version) -> String {
    format!("chore: release {version}")
}

/// The branch a bump is pushed as when the trunk takes no push.
pub fn bump_branch(repo: &str, version: &Version) -> String {
    format!("release/{repo}-{version}")
}

/// Step 4's push of the bump commit `sha`: the trunk pushed, or where the
/// forge refuses it, `sha` pushed as its own branch and squashed into the
/// trunk through a pull request. The clone's trunk then takes the forge's
/// commit, checked to carry the bump's tree on the bump's parent, and that
/// commit is what is returned.
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
        });
    }
    git::reset_hard(root, &landed)?;
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
        });
    }
    git::set_branch(root, setup.release, &landed)?;
    Ok(landed)
}

/// A bump commit a run that stopped after making it left on the trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub sha:     String,
    pub subject: String,
}

/// Before step 1: where the trunk is checked out and clean, one commit past
/// the remote's, and that commit is a bump on the remote's tip, it is set
/// aside, the trunk moved back to its parent, so the check, the gate run and
/// the plan see the commit the gate measured. Anything else is no pending
/// bump and moves nothing.
pub fn set_aside(setup: &Setup<'_>, root: &Path) -> Result<Option<Pending>, ReleaseError> {
    if git::current_branch(root)?.as_deref() != Some(setup.trunk) || !git::is_clean(root)? {
        return Ok(None);
    }
    let Ok(remote) = git::sha(root, &format!("{}/{}", setup.remote, setup.trunk)) else {
        return Ok(None);
    };
    let tip = git::head(root)?;
    if tip == remote
        || git::parent_count(root, &tip)? != 1
        || git::sha(root, &format!("{tip}^"))? != remote
    {
        return Ok(None);
    }
    let subject = git::subject(root, &tip)?;
    if !subject.starts_with("chore: release ") {
        return Ok(None);
    }
    git::reset_hard(root, &remote)?;
    Ok(Some(Pending {
        sha: tip,
        subject,
    }))
}

/// The set-aside bump back on the trunk, after a step before 4 refused.
pub fn put_back(root: &Path, pending: Option<&Pending>) {
    if let Some(p) = pending {
        let _ = git::reset_hard(root, &p.sha);
    }
}

/// Step 4 with a pending bump: taken back where it is the plan's version,
/// and refused, the bump put back, where it is another.
pub fn take_back(root: &Path, pending: &Pending, next: &Version) -> Result<String, ReleaseError> {
    if pending.subject != bump_subject(next) {
        put_back(root, Some(pending));
        return Err(ReleaseError::PendingBump {
            found:   pending.subject.clone(),
            planned: next.to_string(),
        });
    }
    git::fast_forward(root, &pending.sha)?;
    Ok(pending.sha.clone())
}
