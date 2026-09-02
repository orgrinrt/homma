//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release itself, in the nine steps of the deep dive, stopping at the
//! first refusal. Each step is a function so a test can call one alone.

use std::fmt;
use std::path::Path;

use homma_api::{Finding, GateRun, Level, Verdict};

use super::check::{self, Published};
use super::gate::Runner;
use super::git::GitError;
use super::plan::{self, Plan, PlanError};
use super::publish::{self, PublishError, Served, TokenSource};
use super::registry::Registry;
use super::{badges, changelog, git, version};
use crate::forge::{Forge, ForgeError};

/// Everything a release needs to know that is not in the tree.
pub struct Setup<'a> {
    pub runner:    &'a dyn Runner,
    pub forge:     &'a dyn Forge,
    pub owner:     &'a str,
    pub name:      &'a str,
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
    level: Level,
) -> Result<Vec<Finding>, ReleaseError> {
    let findings = check::check(&check::Inputs {
        root,
        remote: setup.remote,
        trunk: setup.trunk,
        release: setup.release,
        level: Some(level),
        published: setup.published,
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
/// commit, and push it.
pub fn step_bump(setup: &Setup<'_>, root: &Path, plan: &Plan) -> Result<String, ReleaseError> {
    match git::current_branch(root)? {
        Some(b) if b == setup.trunk => {},
        other => return Err(ReleaseError::NotOnTrunk(other)),
    }
    let mut paths = vec!["CHANGELOG.md"];
    if plan.current != plan.next {
        version::write(root, plan.repo_kind, &plan.next)?;
        if plan.repo_kind.has_crate() {
            paths.push("Cargo.toml");
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
    let sha = git::commit_paths(root, &paths, &format!("chore: release {}", plan.next))?;
    git::push(root, setup.remote, setup.trunk, false)?;
    Ok(sha)
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
        // reported, so a failed release leaves the branch it borrowed
        let merged = git::merge_no_ff(root, setup.trunk, &format!("release: {}", plan.next))
            .and_then(|sha| git::push(root, setup.remote, setup.release, false).map(|_| sha));
        match merged {
            Ok(sha) => sha,
            Err(e) => {
                let _ = git::abort_merge(root);
                let _ = git::switch(root, setup.trunk);
                return Err(e.into());
            },
        }
    };
    let tagged = git::tag_annotated(root, &plan.tag, &sha, &plan.tag).and_then(|_| {
        git::push(
            root,
            setup.remote,
            &format!("refs/tags/{}", plan.tag),
            false,
        )
    });
    if setup.trunk != setup.release {
        let back = git::switch(root, setup.trunk);
        tagged?;
        back?;
    } else {
        tagged?;
    }
    Ok(sha)
}

/// Step 7: the forge release on the tag, with the block as its body.
pub fn step_forge_release(setup: &Setup<'_>, plan: &Plan) -> Result<(), ReleaseError> {
    setup.forge.create_release(
        setup.owner,
        setup.name,
        &plan.tag,
        plan.changelog.trim_end(),
    )?;
    Ok(())
}

/// Step 8: publish everything the plan names, in its order.
pub fn step_publish(setup: &Setup<'_>, root: &Path, plan: &Plan) -> Result<(), ReleaseError> {
    for (reg, name) in &plan.publishes {
        match reg {
            Registry::CratesIo => {
                publish::publish_crate(
                    setup.runner,
                    root,
                    name,
                    &plan.next,
                    setup.token,
                    setup.served,
                )?
            },
            Registry::Jsr => {
                publish::publish_jsr(
                    setup.runner,
                    root,
                    name,
                    &plan.next,
                    setup.token,
                    setup.served,
                )?
            },
            Registry::Npm => {
                publish::publish_npm(
                    setup.runner,
                    root,
                    name,
                    &plan.next,
                    setup.token,
                    setup.served,
                )?
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
    level: Level,
    newest_run: Option<&GateRun>,
    dry_run: bool,
) -> Result<Result<Released, Plan>, ReleaseError> {
    step_check(setup, root, level)?;
    step_gate_run(root, setup.trunk, newest_run)?;
    let plan = plan::plan(root, setup.trunk, level, setup.date)?;
    if dry_run {
        return Ok(Err(plan));
    }
    let run = newest_run.expect("step two established it");
    let bump_sha = step_bump(setup, root, &plan)?;
    let tag_sha = step_merge_and_tag(setup, root, &plan)?;
    step_forge_release(setup, &plan)?;
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
