//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! How the release lands on the forge. The working trunk takes a push where
//! it takes one; where the forge refuses the push outright, the run lands the
//! same commit through a pull request, merged with a merge commit and never a
//! squash, and waits for it. The release, the trunk into the release line, is
//! a pull request in the same way where the forge refuses the push of the
//! release line. The bump a run that stopped left, on the trunk or already on
//! the forge, whichever pull request landed it, is set aside and taken back
//! when it runs again.

use std::path::Path;

use homma_api::Version;

use super::git::{self, GitError};
use super::run::{ReleaseError, Setup};

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

/// Step 4's landing of the bump commit `sha` on the trunk, and the trunk's
/// commit that holds it, which is returned: `sha` itself where the remote's
/// trunk is at it already or takes the push, else the merge commit of a pull
/// request. Where the forge refuses the push outright, `sha` is pushed as its
/// own branch and landed into the trunk through `ReleaseForge::land`, a merge
/// commit and never a squash, which returns once the forge has merged it. The
/// remote's trunk is then read back and taken only where it is a merge of `sha`
/// onto the commit `sha` was made on, with the tree the two make together, the
/// tree the gate measured; the clone's trunk moves onto it and the branch is
/// dropped. A forge that takes no pull request leaves the commit on the clone's
/// trunk and the branch it pushed dropped, and a failed pull request leaves
/// both, for a rerun to take the open one.
pub fn push_bump(
    setup: &Setup<'_>,
    root: &Path,
    sha: &str,
    version: &Version,
) -> Result<String, ReleaseError> {
    if git::fetch_branch(root, setup.remote, setup.trunk)
        .ok()
        .as_deref()
        == Some(sha)
    {
        return Ok(sha.to_string());
    }
    match git::push(root, setup.remote, setup.trunk, false) {
        Ok(()) => return Ok(sha.to_string()),
        Err(e) if refused(&e) => {},
        Err(e) => return Err(e.into()),
    }
    let head = bump_branch(setup.repo, version);
    let before = git::sha(root, &format!("{sha}^"))?;
    git::push(
        root,
        setup.remote,
        &format!("{sha}:refs/heads/{head}"),
        false,
    )?;
    if let Err(e) = setup
        .forge
        .land(root, &head, setup.trunk, &bump_subject(version), sha)
    {
        if matches!(e, ReleaseError::NoPullRequest(_)) {
            let _ = setup.forge.drop_branch(root, &head, sha);
        }
        return Err(e);
    }
    let landed = git::fetch_branch(root, setup.remote, setup.trunk)?;
    if !is_merge_onto(root, &landed, &before, sha)? {
        return Err(ReleaseError::NotLanded {
            base: setup.trunk.to_string(),
            at:   landed,
            tip:  sha.to_string(),
        });
    }
    git::fast_forward(root, &landed)?;
    let _ = setup.forge.drop_branch(root, &head, sha);
    Ok(landed)
}

/// Whether `merge` is a merge commit of `bump` onto `onto` holding the tree the
/// two make together.
fn is_merge_onto(root: &Path, merge: &str, onto: &str, bump: &str) -> Result<bool, ReleaseError> {
    Ok(git::parent_count(root, merge)? == 2
        && git::sha(root, &format!("{merge}^1"))? == onto
        && git::sha(root, &format!("{merge}^2"))? == bump
        && measured(root, merge, onto, bump)?)
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
        &tip,
    )?;
    let landed = git::fetch_branch(root, setup.remote, setup.release)?;
    if !is_merge_onto(root, &landed, &before, &tip)? {
        return Err(ReleaseError::NotLanded {
            base: setup.release.to_string(),
            at: landed,
            tip,
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
    pub sha:     String,
    /// The version its subject names.
    pub version: Version,
    pub landed:  Landed,
}

/// The version a bump's subject names: `chore: release <version>` with a
/// version that reads as one.
fn subject_version(subject: &str) -> Option<Version> {
    subject
        .strip_prefix(BUMP)
        .and_then(|v| v.parse::<Version>().ok())
}

/// The version a bump commit carries: one parent, and the subject
/// `chore: release <version>` with a version that reads as one.
fn bump_version(root: &Path, rev: &str) -> Result<Option<Version>, ReleaseError> {
    if git::parent_count(root, rev)? != 1 {
        return Ok(None);
    }
    Ok(subject_version(&git::subject(root, rev)?))
}

/// The version of a bump a pull request landed as a merge commit: two parents,
/// the same subject, and a second parent that is a bump of that version.
fn landed_version(root: &Path, rev: &str) -> Result<Option<Version>, ReleaseError> {
    if git::parent_count(root, rev)? != 2 {
        return Ok(None);
    }
    let Some(version) = subject_version(&git::subject(root, rev)?) else {
        return Ok(None);
    };
    Ok((bump_version(root, &format!("{rev}^2"))?.as_ref() == Some(&version)).then_some(version))
}

/// The bump the remote's trunk `merge` landed by a pull request, set aside: the
/// clone's trunk is at the merge, at its first parent or at the bump itself,
/// the merge holds the tree the two make together, and no tag holds it yet. The
/// trunk is moved back to the first parent, the commit the gate measured.
fn set_aside_landed(
    setup: &Setup<'_>,
    root: &Path,
    merge: &str,
    tip: &str,
) -> Result<Option<Pending>, ReleaseError> {
    let Some(version) = landed_version(root, merge)? else {
        return Ok(None);
    };
    let first = git::sha(root, &format!("{merge}^1"))?;
    let bump = git::sha(root, &format!("{merge}^2"))?;
    if ![merge, first.as_str(), bump.as_str()].contains(&tip)
        || !is_merge_onto(root, merge, &first, &bump)?
        || git::tagged(root, merge)?
    {
        return Ok(None);
    }
    let landed = release_merge(setup, root, merge)?;
    git::reset_hard(root, &first)?;
    Ok(Some(Pending {
        sha: merge.to_string(),
        version,
        landed,
    }))
}

/// Before step 1: the trunk and the release line fetched, and where the
/// checked-out, clean trunk ends in a bump nothing tagged yet, it is set
/// aside, the trunk moved back to its parent, so the check, the gate run and
/// the plan see the commit the gate measured. The bump is the clone's own,
/// one past the remote's tip; or the remote's tip itself, pushed by a run that
/// stopped before the tag; or the merge commit of a pull request that landed
/// one, the run's own or another's, which is the remote's tip with the clone at
/// it, at its first parent or at the bump; and where the release line's first
/// parents hold a merge of it, that merge too. Anything else is no pending
/// bump and moves nothing.
pub fn set_aside(setup: &Setup<'_>, root: &Path) -> Result<Option<Pending>, ReleaseError> {
    if git::current_branch(root)?.as_deref() != Some(setup.trunk) || !git::is_clean(root)? {
        return Ok(None);
    }
    let Ok(remote) = git::fetch_branch(root, setup.remote, setup.trunk) else {
        return Ok(None);
    };
    let tip = git::head(root)?;
    if let Some(landed) = set_aside_landed(setup, root, &remote, &tip)? {
        return Ok(Some(landed));
    }
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
        }));
    }
    if tip != remote || git::tagged(root, &tip)? {
        return Ok(None);
    }
    let landed = release_merge(setup, root, &tip)?;
    git::reset_hard(root, &parent)?;
    Ok(Some(Pending {
        sha: tip,
        version,
        landed,
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
/// another version than the plan's; else taken back, and pushed or landed
/// where only the clone held it. A merge a pull request landed has its branch
/// dropped where the remote still holds it.
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
    match pending.landed {
        Landed::Local => push_bump(setup, root, &pending.sha, next),
        _ => {
            drop_landed_branch(setup, root, &pending.sha, next);
            Ok(pending.sha.clone())
        },
    }
}

/// The branch a bump's pull request came from, dropped through the forge where
/// `merge` is that pull request's merge commit and the remote still holds the
/// branch. A failure does not stop the run: the route queued the deletion
/// where it could.
fn drop_landed_branch(setup: &Setup<'_>, root: &Path, merge: &str, version: &Version) {
    if git::parent_count(root, merge).ok() != Some(2) {
        return;
    }
    let branch = bump_branch(setup.repo, version);
    if let Ok(head) = git::fetch_branch(root, setup.remote, &branch) {
        let _ = setup.forge.drop_branch(root, &branch, &head);
    }
}
