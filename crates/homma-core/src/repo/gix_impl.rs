//! `GixRepo` — the [`super::RepoOps`] impl backed by the `gix` crate.
//!
//! Construction surface is impl-specific: [`GixRepo::open`] for an
//! existing local repo, [`GixRepo::clone_into`] for a default-branch
//! clone, [`GixRepo::mirror_into`] for the curated canonical-git mirror
//! used by the migration flow.
//!
//! NOTE: method bodies are stubs at this scaffolding stage; impls land
//! in subsequent commits on this branch.

use std::path::{Path, PathBuf};

use super::error::RepoError;
use super::ops::{Branch, MirrorOpts, Remote, RepoOps, Status};

/// gix-backed [`RepoOps`] implementation.
pub struct GixRepo {
    handle: gix::Repository,
    root: PathBuf,
}

impl GixRepo {
    /// Open an existing local repository at `path`.
    pub fn open(path: &Path) -> Result<Self, RepoError> {
        let _ = path;
        todo!("GixRepo::open via gix::open")
    }

    /// Standard clone: fetch the default branch and check it out at `dest`.
    pub fn clone_into(url: &str, dest: &Path) -> Result<Self, RepoError> {
        let _ = (url, dest);
        todo!("GixRepo::clone_into via gix::prepare_clone + fetch + main_worktree")
    }

    /// Curated mirror clone: fetch the refspecs in `opts` directly under
    /// the local `refs/` tree. Forge-specific refs are excluded by the
    /// default `MirrorOpts`; see `canonical_refspecs`.
    pub fn mirror_into(
        url: &str,
        dest: &Path,
        opts: MirrorOpts,
    ) -> Result<Self, RepoError> {
        let _ = (url, dest, opts);
        todo!("GixRepo::mirror_into via configure_remote + custom refspecs")
    }

    /// Access the underlying `gix::Repository`. Escape hatch for callers
    /// that need a backend-specific operation not yet covered by `RepoOps`.
    pub fn handle(&self) -> &gix::Repository {
        &self.handle
    }

    /// Local filesystem root of the repository.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl RepoOps for GixRepo {
    fn status(&self) -> Result<Status, RepoError> {
        todo!("GixRepo::status via is_dirty + status iter + ahead/behind revwalk")
    }

    fn current_branch(&self) -> Result<Option<String>, RepoError> {
        todo!("GixRepo::current_branch via head_name")
    }

    fn branches(&self) -> Result<Vec<Branch>, RepoError> {
        todo!("GixRepo::branches via references filtered to refs/heads/*")
    }

    fn remotes(&self) -> Result<Vec<Remote>, RepoError> {
        todo!("GixRepo::remotes via remote_names + find_remote per name")
    }

    fn add_remote(&mut self, name: &str, url: &str) -> Result<(), RepoError> {
        let _ = (name, url);
        todo!("GixRepo::add_remote via config_snapshot_mut")
    }

    fn remove_remote(&mut self, name: &str) -> Result<(), RepoError> {
        let _ = name;
        todo!("GixRepo::remove_remote via config_snapshot_mut")
    }

    fn checkout(&mut self, branch: &str) -> Result<(), RepoError> {
        let _ = branch;
        todo!("GixRepo::checkout via head_ref reset + worktree update")
    }

    fn create_branch(&mut self, name: &str, from: &str) -> Result<(), RepoError> {
        let _ = (name, from);
        todo!("GixRepo::create_branch via reference_create")
    }
}
