//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The two writes a release makes on its forge, the annotated tag on the
//! release commit and the release on that tag, as one contract with an
//! implementation for each way they reach the forge.

use std::path::Path;

use super::gate::Runner;
use super::git;
use super::run::ReleaseError;
use crate::forge::Forge;

/// The forge writes of steps 6 and 7. The run holds one of these and never
/// asks which way the writes go.
pub trait ReleaseForge {
    /// The annotated tag `tag` on `sha`, with `message`, made on the remote.
    fn tag(&self, root: &Path, tag: &str, sha: &str, message: &str) -> Result<(), ReleaseError>;

    /// The forge release on `tag`, with `body` as its notes.
    fn release(&self, root: &Path, tag: &str, body: &str) -> Result<(), ReleaseError>;
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
}
