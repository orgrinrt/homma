//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The writes a release makes on its forge, the annotated tag on the release
//! commit, the release on that tag, and the pull request that lands a push the
//! forge refused, as one contract with an implementation for each way they
//! reach the forge.

use std::path::Path;

use super::gate::Runner;
use super::git;
use super::run::ReleaseError;
use crate::forge::Forge;

/// How the forge merges a pull request it lands: the bump is squashed into
/// the trunk under its own subject, and the release line takes a merge commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    Squash,
    Merge,
}

impl Merge {
    /// The word the route takes for it.
    pub const fn word(self) -> &'static str {
        match self {
            Merge::Squash => "squash",
            Merge::Merge => "merge",
        }
    }
}

/// The forge writes of steps 4 to 7. The run holds one of these and never
/// asks which way the writes go.
pub trait ReleaseForge {
    /// The annotated tag `tag` on `sha`, with `message`, made on the remote.
    fn tag(&self, root: &Path, tag: &str, sha: &str, message: &str) -> Result<(), ReleaseError>;

    /// The forge release on `tag`, with `body` as its notes.
    fn release(&self, root: &Path, tag: &str, body: &str) -> Result<(), ReleaseError>;

    /// `sha`, pushed as `head`, landed into `base` through a pull request
    /// the forge merges by `how` at that head, `title` its subject and no
    /// body. The open one from `head` into `base` is taken where there is one.
    fn land(
        &self,
        root: &Path,
        head: &str,
        base: &str,
        title: &str,
        how: Merge,
        sha: &str,
    ) -> Result<(), ReleaseError>;
}

/// Git tags in the clone and pushes the tag to `remote`; the forge client
/// makes the release on `owner`/`name`.
pub struct ByClient<'a> {
    pub forge:  &'a dyn Forge,
    pub owner:  &'a str,
    pub name:   &'a str,
    pub remote: &'a str,
}

impl ReleaseForge for ByClient<'_> {
    fn tag(&self, root: &Path, tag: &str, sha: &str, message: &str) -> Result<(), ReleaseError> {
        git::tag_annotated(root, tag, sha, message)?;
        git::push(root, self.remote, &format!("refs/tags/{tag}"), false)?;
        Ok(())
    }

    fn release(&self, _root: &Path, tag: &str, body: &str) -> Result<(), ReleaseError> {
        self.forge
            .create_release(self.owner, self.name, tag, body)?;
        Ok(())
    }

    /// The client opens no pull request: that is the route's, so a push the
    /// forge refused stays refused, named.
    fn land(
        &self,
        _root: &Path,
        _head: &str,
        base: &str,
        _title: &str,
        _how: Merge,
        _sha: &str,
    ) -> Result<(), ReleaseError> {
        Err(ReleaseError::NoPullRequest(base.to_string()))
    }
}

/// The workspace's `forge_route`, absolute, asked for both writes through
/// the release's runner. It reaches GitHub alone, and reads the repo from the
/// clone's origin.
pub struct ByRoute<'a> {
    pub route:  &'a Path,
    pub runner: &'a dyn Runner,
}

impl ByRoute<'_> {
    /// One write asked of the route, in the repo's root. The route tries each
    /// way the machine has and is silent when one worked; where every one
    /// failed its one line is the error.
    fn ask(&self, root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<(), ReleaseError> {
        let out = self
            .runner
            .run(root, &self.route.to_string_lossy(), args, env)
            .map_err(|e| ReleaseError::Route(e.to_string()))?;
        if out.ok() {
            return Ok(());
        }
        let said = out.log();
        Err(ReleaseError::Route(if said.trim().is_empty() {
            out.command_line()
        } else {
            said.trim().to_string()
        }))
    }
}

impl ReleaseForge for ByRoute<'_> {
    fn tag(&self, root: &Path, tag: &str, sha: &str, message: &str) -> Result<(), ReleaseError> {
        let at = root.to_string_lossy();
        self.ask(root, &["tag", &at, tag, sha, message], &[])
    }

    fn release(&self, root: &Path, tag: &str, body: &str) -> Result<(), ReleaseError> {
        let at = root.to_string_lossy();
        self.ask(root, &["release", &at, tag], &[("FR_NOTES", body)])
    }

    fn land(
        &self,
        root: &Path,
        head: &str,
        base: &str,
        title: &str,
        how: Merge,
        sha: &str,
    ) -> Result<(), ReleaseError> {
        let at = root.to_string_lossy();
        self.ask(root, &["pr", &at, head, base, title, how.word(), sha], &[])
    }
}
