//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release itself, in the nine steps of the deep dive, stopping at the
//! first refusal. Each step is a function so a test can call one alone.

use std::fmt;
use std::path::Path;

use homma_api::{Finding, GateRun, Markers, Target, Verdict};

use super::check::{self, Published};
use super::gate::Runner;
use super::git::GitError;
use super::landing::{self, Pending};
use super::plan::{self, Plan, PlanError};
use super::publish::{self, PublishError, Served, TokenSource};
use super::registry::Registry;
use super::workspace::{self, Layout};
use super::writes::ReleaseForge;
use super::{badges, changelog, git, version};
use crate::forge::ForgeError;

/// Everything a release needs to know that is not in the tree.
pub struct Setup<'a> {
    pub runner:    &'a dyn Runner,
    /// The repository as the workspace manifest names it.
    pub repo:      &'a str,
    /// The forge writes of steps 6 and 7, the tag and the release; the engine
    /// picks which way they go for the repo.
    pub forge:     &'a dyn ReleaseForge,
    pub remote:    &'a str,
    /// The working trunk, `dev`.
    pub trunk:     &'a str,
    /// The release line, `main`. Where it equals the trunk there is no
    /// merge, and the tag lands on the bump commit.
    pub release:   &'a str,
    pub date:      &'a str,
    pub token:     TokenSource<'a>,
    pub served:    Served<'a>,
    pub published: &'a Published,
    /// The workspace's root markers, which decide what the repo is.
    pub markers:   &'a Markers,
}

/// Why the release stopped, and at which step.
#[derive(Debug)]
pub enum ReleaseError {
    /// The check found something that blocks.
    Blocked(Vec<Finding>),
    /// No green gate run on the tip of the trunk.
    NoGreenRun {
        sha:   String,
        found: Option<Verdict>,
    },
    /// The tree is not on the trunk.
    NotOnTrunk(Option<String>),
    Plan(PlanError),
    Git(GitError),
    Version(version::VersionError),
    Io(std::io::Error),
    Forge(ForgeError),
    Publish(PublishError),
    /// The workspace's forge route made neither the tag nor the release by
    /// any of its routes, or could not be run; its own words.
    Route(String),
    /// The tag the remote holds names another commit than the one the
    /// release tagged.
    TagElsewhere {
        tag: String,
        at:  String,
        sha: String,
    },
    /// The forge refused a push to the branch, and the forge writes in use
    /// open no pull request.
    NoPullRequest(String),
    /// After a pull request into `base` was landed, the remote's `base` at
    /// `at` is not what the release pushed there.
    NotLanded {
        base: String,
        at:   String,
    },
    /// A bump a stopped run left on the trunk is for another version than
    /// the plan.
    PendingBump {
        found:   String,
        planned: String,
    },
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReleaseError::Blocked(findings) => {
                writeln!(f, "the check blocks the release:")?;
                for x in findings.iter().filter(|x| x.severity.blocks()) {
                    writeln!(f, "  {} {}", x.id, x.message)?;
                }
                Ok(())
            },
            ReleaseError::NoGreenRun {
                sha,
                found,
            } => {
                match found {
                    Some(v) => {
                        write!(
                            f,
                            "the newest gate run on {sha} is {v}; a green one is needed"
                        )
                    },
                    None => {
                        write!(
                            f,
                            "no gate run recorded on {sha}; push it through the hook or run `homma release gate`"
                        )
                    },
                }
            },
            ReleaseError::NotOnTrunk(b) => {
                write!(
                    f,
                    "the tree is on {}, and a release runs from the trunk",
                    b.as_deref().unwrap_or("a detached head")
                )
            },
            ReleaseError::Plan(e) => write!(f, "{e}"),
            ReleaseError::Git(e) => write!(f, "{e}"),
            ReleaseError::Version(e) => write!(f, "{e}"),
            ReleaseError::Io(e) => write!(f, "{e}"),
            ReleaseError::Forge(e) => write!(f, "{e}"),
            ReleaseError::Publish(e) => write!(f, "{e}"),
            ReleaseError::Route(said) => write!(f, "{said}"),
            ReleaseError::TagElsewhere {
                tag,
                at,
                sha,
            } => {
                write!(
                    f,
                    "the remote's {tag} names {at}, and the release tagged {sha}"
                )
            },
            ReleaseError::NoPullRequest(base) => {
                write!(
                    f,
                    "the forge takes no push to {base}, only a pull request, and those go through the workspace's `forge_route`, which it does not name"
                )
            },
            ReleaseError::NotLanded {
                base,
                at,
            } => {
                write!(
                    f,
                    "the pull request into {base} left it at {at}, which is not what the release pushed"
                )
            },
            ReleaseError::PendingBump {
                found,
                planned,
            } => {
                write!(
                    f,
                    "the trunk carries `{found}` from a run that stopped, and this run plans {planned}; it is left on the trunk"
                )
            },
        }
    }
}

impl std::error::Error for ReleaseError {}

macro_rules! from_err {
    ($($t:ty => $v:ident),*) => {$(
        impl From<$t> for ReleaseError {
            fn from(e: $t) -> Self { ReleaseError::$v(e) }
        }
    )*};
}

from_err!(PlanError => Plan, GitError => Git, version::VersionError => Version, std::io::Error => Io, ForgeError => Forge, PublishError => Publish);

/// What a finished release did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Released {
    pub plan:       Plan,
    pub bump_sha:   String,
    pub tag_sha:    String,
    pub badges_sha: String,
}

/// Step 1: the check, refusing on anything blocking.
pub fn step_check(
    setup: &Setup<'_>,
    root: &Path,
    target: &Target,
) -> Result<Vec<Finding>, ReleaseError> {
    let findings = check::check(&check::Inputs {
        root,
        repo: setup.repo,
        remote: setup.remote,
        trunk: setup.trunk,
        release: setup.release,
        target: Some(target.clone()),
        published: setup.published,
        markers: setup.markers,
    })?;
    if check::blocked(&findings) {
        return Err(ReleaseError::Blocked(findings));
    }
    Ok(findings)
}

/// Step 2: a green gate run on exactly the trunk's tip.
pub fn step_gate_run(
    root: &Path,
    trunk: &str,
    newest: Option<&GateRun>,
) -> Result<(), ReleaseError> {
    let sha = git::sha(root, trunk)?;
    match newest {
        Some(run) if run.sha == sha && run.verdict == Verdict::Green => Ok(()),
        Some(run) if run.sha == sha => {
            Err(ReleaseError::NoGreenRun {
                sha,
                found: Some(run.verdict),
            })
        },
        _ => {
            Err(ReleaseError::NoGreenRun {
                sha,
                found: None,
            })
        },
    }
}

/// Step 4: bump the manifest and prepend the changelog on the trunk, as one
/// commit, and push it, through a pull request where the forge refuses the
/// push. A `pending` bump a stopped run left is taken back instead of a
/// second being written. Returns the bump as the trunk holds it.
pub fn step_bump(
    setup: &Setup<'_>,
    root: &Path,
    plan: &Plan,
    pending: Option<&Pending>,
) -> Result<String, ReleaseError> {
    match git::current_branch(root)? {
        Some(b) if b == setup.trunk => {},
        other => return Err(ReleaseError::NotOnTrunk(other)),
    }
    let sha = match pending {
        Some(p) => landing::take_back(root, p, &plan.next)?,
        None => write_bump(setup, root, plan)?,
    };
    landing::push_bump(setup, root, &sha, &plan.next)
}

/// The bump commit itself: the manifest and the changelog, as one commit.
fn write_bump(setup: &Setup<'_>, root: &Path, plan: &Plan) -> Result<String, ReleaseError> {
    let mut paths = vec!["CHANGELOG.md"];
    if plan.current != plan.next {
        match &plan.layout {
            Layout::Single => version::write(root, plan.repo_kind, &plan.next)?,
            Layout::Set {
                primary,
                ..
            } => workspace::write_version(root, primary, &plan.next)?,
        }
        if plan.repo_kind.has_crate() {
            paths.push(match &plan.layout {
                Layout::Single => "Cargo.toml",
                Layout::Set {
                    primary,
                    ..
                } => primary.manifest.as_str(),
            });
            if root.join("Cargo.lock").is_file() {
                // the lock names the workspace's own versions
                let _ =
                    setup
                        .runner
                        .run(root, "cargo", &["update", "--workspace", "--offline"], &[]);
                paths.push("Cargo.lock");
            }
        }
        if plan.repo_kind.has_deno() {
            paths.push("deno.json");
        }
    }
    changelog::prepend(root, &plan.changelog)?;
    Ok(git::commit_paths(
        root,
        &paths,
        &landing::bump_subject(&plan.next),
    )?)
}

/// Steps 5 and 6: merge the trunk into the release line with a merge commit
/// and tag that, or tag the bump where the two are one branch; push both.
pub fn step_merge_and_tag(
    setup: &Setup<'_>,
    root: &Path,
    plan: &Plan,
) -> Result<String, ReleaseError> {
    let sha = if setup.trunk == setup.release {
        git::head(root)?
    } else {
        git::switch(root, setup.release)?;
        // a failure here hands the tree back to the trunk before it is
        // reported, and the release line back to where it was: a merge that
        // landed and did not push cannot be aborted, only reset, and left
        // there it makes the next run refuse on the wrong cause
        let before = git::head(root)?;
        let merged = git::merge_no_ff(root, setup.trunk, &format!("release: {}", plan.next))
            .and_then(|sha| git::push(root, setup.remote, setup.release, false).map(|_| sha));
        match merged {
            Ok(sha) => sha,
            Err(e) => {
                let _ = git::abort_merge(root);
                let _ = git::reset_hard(root, &before);
                let _ = git::switch(root, setup.trunk);
                if !landing::refused(&e) {
                    return Err(e.into());
                }
                // the release line takes changes only through a pull request:
                // the release pull request makes the same merge on the forge
                landing::land_release(setup, root, &plan.next)?
            },
        }
    };
    let tagged = setup
        .forge
        .tag(root, &plan.tag, &sha, &plan.tag)
        .and_then(|_| step_hold_tag(setup, root, &plan.tag, &sha));
    if setup.trunk != setup.release {
        let back = git::switch(root, setup.trunk);
        tagged?;
        back?;
    } else {
        tagged?;
    }
    Ok(sha)
}

/// After step 6: the tag the remote now holds, read from the remote and
/// refused where it names another commit than `sha`, then written into the
/// clone where the clone does not hold it yet. A tag made on the remote alone,
/// as a route makes it, would leave the clone's newest version tag the one
/// before, and the next plan would release this version again; and a tag on
/// the wrong commit is compared before it is written, so a refusal leaves the
/// clone no version tag on the wrong commit.
pub fn step_hold_tag(
    setup: &Setup<'_>,
    root: &Path,
    tag: &str,
    sha: &str,
) -> Result<(), ReleaseError> {
    let elsewhere = |at: String| {
        ReleaseError::TagElsewhere {
            tag: tag.to_string(),
            at,
            sha: sha.to_string(),
        }
    };
    let object = git::fetch_tag_object(root, setup.remote, tag)?;
    let at = git::sha(root, &object)?;
    if at != sha {
        return Err(elsewhere(at));
    }
    match git::tag_target(root, tag) {
        Ok(local) if local == sha => Ok(()),
        Ok(local) => Err(elsewhere(local)),
        Err(_) => Ok(git::create_tag_ref(root, tag, &object)?),
    }
}

/// Step 7: the forge release on the tag, with the block as its body.
pub fn step_forge_release(setup: &Setup<'_>, root: &Path, plan: &Plan) -> Result<(), ReleaseError> {
    setup
        .forge
        .release(root, &plan.tag, plan.changelog.trim_end())
}

/// Step 8: publish everything the plan names, in its order.
pub fn step_publish(setup: &Setup<'_>, root: &Path, plan: &Plan) -> Result<(), ReleaseError> {
    for (reg, name, version) in &plan.publishes {
        match reg {
            Registry::CratesIo => {
                publish::publish_crate(
                    setup.runner,
                    root,
                    name,
                    version,
                    setup.token,
                    setup.served,
                )?
            },
            Registry::Jsr => {
                publish::publish_jsr(setup.runner, root, name, version, setup.token, setup.served)?
            },
            Registry::Npm => {
                publish::publish_npm(setup.runner, root, name, version, setup.token, setup.served)?
            },
        }
    }
    Ok(())
}

/// Step 9: the badges branch, from the run that gated the release.
pub fn step_badges(
    setup: &Setup<'_>,
    root: &Path,
    run: &GateRun,
    plan: &Plan,
) -> Result<String, ReleaseError> {
    let files = badges::files(run, &plan.next);
    let sha = badges::write(root, &files)?;
    git::push(
        root,
        setup.remote,
        &format!("refs/heads/{}", badges::BRANCH),
        true,
    )?;
    Ok(sha)
}

/// The whole release. `newest_run` is the newest recorded gate run for the
/// repo, which the caller reads out of the store.
pub fn release(
    setup: &Setup<'_>,
    root: &Path,
    target: impl Into<Target>,
    newest_run: Option<&GateRun>,
    dry_run: bool,
) -> Result<Result<Released, Plan>, ReleaseError> {
    let target = target.into();
    // a bump a stopped run left is set aside, so the first three steps see
    // the commit the gate measured, and put back wherever the run stops short
    // of taking it
    let pending = landing::set_aside(setup, root)?;
    let planned = step_check(setup, root, &target)
        .and_then(|_| step_gate_run(root, setup.trunk, newest_run))
        .and_then(|_| {
            Ok(plan::plan(
                root,
                setup.repo,
                setup.markers,
                setup.trunk,
                target,
                setup.date,
            )?)
        });
    let plan = match planned {
        Ok(plan) if !dry_run => plan,
        other => {
            landing::put_back(root, pending.as_ref());
            return other.map(Err);
        },
    };
    let run = newest_run.expect("step two established it");
    let bump_sha = step_bump(setup, root, &plan, pending.as_ref())?;
    let tag_sha = step_merge_and_tag(setup, root, &plan)?;
    step_forge_release(setup, root, &plan)?;
    step_publish(setup, root, &plan)?;
    let badges_sha = step_badges(setup, root, run, &plan)?;
    Ok(Ok(Released {
        plan,
        bump_sha,
        tag_sha,
        badges_sha,
    }))
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
