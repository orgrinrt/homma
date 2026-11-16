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
    drop_squashed(setup, root, &head, sha);
    Ok(landed)
}

/// Step 5 where the release line refused the push of the local merge, which
/// the caller has undone: the release pull request, the trunk into the
/// release line, merged by the forge with a merge commit at the trunk's tip.
/// The clone's release line takes that commit once it is checked to be a
/// merge of the tip onto where the line was with the tree the two make
/// together, the tree the gate measured, and it is returned for the tag.
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
        || !measured(root, &landed, &before, &tip)?
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

/// Whether `merge` holds the tree of `bump` merged onto `onto`, the tree the
/// gate measured, so a merge the forge resolved another way, or made onto
/// something else that landed first, is never tagged.
fn measured(root: &Path, merge: &str, onto: &str, bump: &str) -> Result<bool, ReleaseError> {
    let want = git::merge_tree(root, onto, bump)?;
    Ok(git::tree(root, merge)? == want)
}

/// The squashed bump's branch dropped through the forge. A failure does not
/// stop the run: the route queued the deletion where it could, and a rerun
/// on the forge's squash asks again.
fn drop_squashed(setup: &Setup<'_>, root: &Path, branch: &str, head: &str) {
    let _ = setup.forge.drop_branch(root, branch, head);
}

/// Where a bump a stopped run left stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landed {
    /// The clone alone holds it, and step 4 pushes it.
    Local,
    /// The remote's trunk holds it, and step 4 takes it as it is.
    Trunk,
    /// The remote's release line holds this merge of it too, with the tree
    /// the gate measured, and step 5 takes the merge and tags it.
    Release(String),
    /// The remote's release line holds this merge of it with another tree
    /// than the gate measured, and step 5 refuses it, nothing tagged.
    Unmeasured(String),
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
    /// The head `release/<repo>-<version>` was pushed at, where the forge
    /// squashed it into `sha` and may still hold the branch.
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

/// Whether `squash` carries `bump`'s tree on `bump`'s parent, as the forge's
/// squash of it does.
fn squash_of(root: &Path, squash: &str, bump: &str) -> Result<bool, ReleaseError> {
    Ok(
        git::sha(root, &format!("{squash}^"))? == git::sha(root, &format!("{bump}^"))?
            && git::tree(root, squash)? == git::tree(root, bump)?,
    )
}

/// Before step 1: the trunk and the release line fetched, and where the
/// checked-out, clean trunk ends in a bump nothing tagged yet, it is set
/// aside, the trunk moved back to its parent, so the check, the gate run and
/// the plan see the commit the gate measured. The bump is the clone's own,
/// one past the remote's tip; or the remote's tip itself, a pull request that
/// merged unseen, the clone's tip or a squash of it, with the branch the
/// squash came from where the forge still holds one; and where the release
/// line's first parents hold a merge of it, that merge too. Anything else is
/// no pending bump and moves nothing.
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
        // the clone already took the forge's squash: the branch it came from,
        // where the forge still holds it, is still to be dropped
        let branch = bump_branch(setup.repo, &version);
        let head = git::fetch_branch(root, setup.remote, &branch)
            .ok()
            .filter(|h| *h != tip);
        let squashed = match head {
            Some(h) if squash_of(root, &tip, &h)? => Some(h),
            _ => None,
        };
        (tip, squashed)
    } else if bump_version(root, &remote)?.as_ref() == Some(&version)
        && squash_of(root, &remote, &tip)?
    {
        (remote, Some(tip))
    } else {
        return Ok(None);
    };
    if git::tagged(root, &sha)? {
        return Ok(None);
    }
    let landed = release_merge(setup, root, &sha)?;
    git::reset_hard(root, &parent)?;
    Ok(Some(Pending {
        sha,
        version,
        landed,
        squashed,
    }))
}

/// The merge of `bump` on the remote's release line, fetched, where its first
/// parents hold one: taken where its tree is the bump merged onto the release
/// line the clone holds, the one the run saw, and refused where it is another.
fn release_merge(setup: &Setup<'_>, root: &Path, bump: &str) -> Result<Landed, ReleaseError> {
    if setup.trunk == setup.release {
        return Ok(Landed::Trunk);
    }
    let Ok(line) = git::fetch_branch(root, setup.remote, setup.release) else {
        return Ok(Landed::Trunk);
    };
    let Some(merge) = git::merge_of(root, &line, bump)? else {
        return Ok(Landed::Trunk);
    };
    let seen = git::sha(root, setup.release)?;
    Ok(if measured(root, &merge, &seen, bump)? {
        Landed::Release(merge)
    } else {
        Landed::Unmeasured(merge)
    })
}

/// The set-aside bump back on the trunk, after a step before 4 refused.
pub fn put_back(root: &Path, pending: Option<&Pending>) -> Result<(), ReleaseError> {
    let Some(p) = pending else {
        return Ok(());
    };
    git::reset_hard(root, &p.sha).map_err(|e| {
        ReleaseError::NotPutBack {
            sha:     p.sha.clone(),
            why:     e.to_string(),
            stopped: None,
        }
    })
}

/// `outcome` once the set-aside bump is back on the trunk; where it cannot be
/// put back, that is the error, carrying the refusal `outcome` held, so
/// neither goes unsaid.
pub fn put_back_after<T>(
    root: &Path,
    pending: Option<&Pending>,
    outcome: Result<T, ReleaseError>,
) -> Result<T, ReleaseError> {
    match put_back(root, pending) {
        Ok(()) => outcome,
        Err(ReleaseError::NotPutBack {
            sha,
            why,
            ..
        }) => {
            Err(ReleaseError::NotPutBack {
                sha,
                why,
                stopped: outcome.err().map(Box::new),
            })
        },
        Err(other) => Err(other),
    }
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
        return put_back_after(
            root,
            Some(pending),
            Err(ReleaseError::PendingBump {
                found:   bump_subject(&pending.version),
                planned: next.to_string(),
            }),
        );
    }
    git::fast_forward(root, &pending.sha)?;
    match (&pending.landed, &pending.squashed) {
        (Landed::Local, _) => push_bump(setup, root, &pending.sha, next),
        (_, Some(head)) => {
            drop_squashed(setup, root, &bump_branch(setup.repo, next), head);
            Ok(pending.sha.clone())
        },
        (_, None) => Ok(pending.sha.clone()),
    }
}
