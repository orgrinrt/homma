//! `GixRepo` — the [`super::RepoOps`] impl backed by the `gix` crate.
//!
//! Construction surface is impl-specific: [`GixRepo::open`] for an
//! existing local repo, [`GixRepo::clone_into`] for a default-branch
//! clone, [`GixRepo::mirror_into`] for the curated canonical-git mirror
//! used by the migration flow.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gix::bstr::{BStr, ByteSlice};
use gix::progress::Discard;

use super::error::RepoError;
use super::ops::{Branch, MirrorOpts, Remote, RepoOps, Status, TrackingStatus};

/// gix-backed [`RepoOps`] implementation.
pub struct GixRepo {
    handle: gix::Repository,
    root: PathBuf,
}

impl GixRepo {
    /// Open an existing local repository at `path`.
    pub fn open(path: &Path) -> Result<Self, RepoError> {
        let handle = gix::open(path).map_err(|e| RepoError::Open(Box::new(e)))?;
        let root = handle
            .work_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| handle.git_dir().to_path_buf());
        Ok(Self { handle, root })
    }

    /// Standard clone: fetch the default branch and check it out at `dest`.
    pub fn clone_into(url: &str, dest: &Path) -> Result<Self, RepoError> {
        let interrupt = AtomicBool::new(false);
        let mut prepare = gix::prepare_clone(url, dest).map_err(|e| RepoError::Clone(Box::new(e)))?;
        let (mut checkout, _outcome) = prepare
            .fetch_then_checkout(Discard, &interrupt)
            .map_err(|e| RepoError::Fetch(Box::new(e)))?;
        let (handle, _outcome) = checkout
            .main_worktree(Discard, &interrupt)
            .map_err(|e| RepoError::Checkout(Box::new(e)))?;
        let root = handle
            .work_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| handle.git_dir().to_path_buf());
        Ok(Self { handle, root })
    }

    /// Curated mirror clone: fetch the refspecs in `opts` directly under
    /// the local `refs/` tree. Forge-specific refs are excluded by the
    /// default `MirrorOpts`; see [`super::canonical_refspecs`].
    ///
    /// The resulting clone is bare-style (no worktree); the refs are what
    /// the migration push consumes.
    pub fn mirror_into(url: &str, dest: &Path, opts: MirrorOpts) -> Result<Self, RepoError> {
        let interrupt = AtomicBool::new(false);
        let specs = opts.include_refspecs.clone();
        let mut prepare = gix::prepare_clone_bare(url, dest)
            .map_err(|e| RepoError::Clone(Box::new(e)))?
            .configure_remote(move |remote| {
                let bstr_specs: Vec<&BStr> = specs.iter().map(|s| s.as_str().into()).collect();
                let r = remote.with_refspecs(bstr_specs, gix::remote::Direction::Fetch)?;
                Ok(r)
            });
        let (handle, _outcome) = prepare
            .fetch_only(Discard, &interrupt)
            .map_err(|e| RepoError::Fetch(Box::new(e)))?;
        let root = handle.git_dir().to_path_buf();
        Ok(Self { handle, root })
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

fn strip_heads_prefix(full: &BStr) -> String {
    let s = full.to_str_lossy();
    s.strip_prefix("refs/heads/").unwrap_or(&s).to_string()
}

impl RepoOps for GixRepo {
    fn status(&self) -> Result<Status, RepoError> {
        let current_branch = self.current_branch()?;

        let is_clean = self
            .handle
            .is_dirty()
            .map_err(|e| RepoError::Status(e.to_string()))
            .map(|dirty| !dirty)?;

        // uncommitted_changes count: drive the status iterator and count entries.
        // The count is best-effort; if the iterator setup fails we fall back to
        // a binary clean/dirty result.
        let uncommitted_changes = if is_clean {
            0
        } else {
            self.count_index_worktree_changes().unwrap_or(0)
        };

        let tracking = self.compute_tracking_status()?;

        Ok(Status {
            current_branch,
            is_clean,
            uncommitted_changes,
            tracking,
        })
    }

    fn current_branch(&self) -> Result<Option<String>, RepoError> {
        let head = self
            .handle
            .head_name()
            .map_err(|e| RepoError::References(e.to_string()))?;
        Ok(head.map(|full| strip_heads_prefix(full.as_ref().as_bstr())))
    }

    fn branches(&self) -> Result<Vec<Branch>, RepoError> {
        let refs = self
            .handle
            .references()
            .map_err(|e| RepoError::References(e.to_string()))?;
        let iter = refs
            .local_branches()
            .map_err(|e| RepoError::References(e.to_string()))?;
        let mut out = Vec::new();
        for r in iter {
            let mut r = r.map_err(|e| RepoError::References(e.to_string()))?;
            let name = strip_heads_prefix(r.name().as_bstr());
            let head_commit = r
                .peel_to_id_in_place()
                .map_err(|e| RepoError::References(e.to_string()))?
                .to_string();
            out.push(Branch { name, head_commit });
        }
        Ok(out)
    }

    fn remotes(&self) -> Result<Vec<Remote>, RepoError> {
        let names = self.handle.remote_names();
        let mut out = Vec::new();
        for name in names {
            let remote = self
                .handle
                .find_remote(name.as_ref())
                .map_err(|e| RepoError::Remote(e.to_string()))?;
            let url = remote
                .url(gix::remote::Direction::Fetch)
                .map(|u| u.to_bstring().to_string())
                .unwrap_or_default();
            out.push(Remote {
                name: name.to_string(),
                url,
            });
        }
        Ok(out)
    }

    fn add_remote(&mut self, name: &str, url: &str) -> Result<(), RepoError> {
        // Write `remote.<name>.url` and `remote.<name>.fetch` directly to
        // the underlying config file. Going through `Remote::save_as_to`
        // would force us to hold an immutable borrow on `self.handle`
        // across the mutable config snapshot; manual section composition
        // sidesteps that constraint and reads cleaner besides.
        gix::remote::name::validated(name)
            .map_err(|e| RepoError::Remote(e.to_string()))?;
        let snapshot = self.handle.config_snapshot_mut();
        let mut file = snapshot.forget();
        if let Some(existing) = file.remove_section("remote", Some(name.into())) {
            // Drop any pre-existing block so we don't end up with duplicates.
            let _ = existing;
        }
        let mut section = file
            .new_section("remote", Some(std::borrow::Cow::Owned(name.into())))
            .map_err(|e| RepoError::Remote(e.to_string()))?;
        let key_url = gix::config::parse::section::ValueName::try_from("url")
            .map_err(|e| RepoError::Remote(e.to_string()))?;
        section.push(key_url, Some(url.into()));
        let fetch_spec = format!("+refs/heads/*:refs/remotes/{name}/*");
        let key_fetch = gix::config::parse::section::ValueName::try_from("fetch")
            .map_err(|e| RepoError::Remote(e.to_string()))?;
        section.push(key_fetch, Some(fetch_spec.as_str().into()));
        write_config_to_disk(&self.handle, &file)?;
        self.reload()?;
        Ok(())
    }

    fn remove_remote(&mut self, name: &str) -> Result<(), RepoError> {
        let snapshot = self.handle.config_snapshot_mut();
        let mut file = snapshot.forget();
        let removed = file.remove_section("remote", Some(name.into()));
        if removed.is_none() {
            return Err(RepoError::Remote(format!("remote not found: {name}")));
        }
        write_config_to_disk(&self.handle, &file)?;
        self.reload()?;
        Ok(())
    }

    fn checkout(&mut self, branch: &str) -> Result<(), RepoError> {
        let full = format!("refs/heads/{branch}");
        // Verify the branch exists.
        self.handle
            .find_reference(full.as_str())
            .map_err(|_| RepoError::BranchNotFound(branch.to_string()))?;

        // Update HEAD to point symbolically at the branch.
        let head_path = self.handle.git_dir().join("HEAD");
        std::fs::write(&head_path, format!("ref: {full}\n")).map_err(|source| RepoError::Io {
            path: head_path,
            source,
        })?;
        self.reload()?;
        // Note: worktree files are not updated here. Callers that need the
        // working tree to reflect the new branch should perform a follow-up
        // checkout via the git CLI or wait for a future worktree-update API.
        Ok(())
    }

    fn create_branch(&mut self, name: &str, from: &str) -> Result<(), RepoError> {
        // Resolve `from` to a commit object id. Accept either an existing
        // branch name (refs/heads/<from>) or any rev-parseable spec.
        let id = self
            .handle
            .rev_parse_single(from)
            .map_err(|e| RepoError::References(format!("resolve `{from}`: {e}")))?
            .detach();

        let full = format!("refs/heads/{name}");
        self.handle
            .reference(
                full.as_str(),
                id,
                gix::refs::transaction::PreviousValue::MustNotExist,
                format!("create branch {name} from {from}"),
            )
            .map_err(|e| RepoError::References(e.to_string()))?;
        Ok(())
    }
}

impl GixRepo {
    fn reload(&mut self) -> Result<(), RepoError> {
        self.handle = gix::open(&self.root).map_err(|e| RepoError::Open(Box::new(e)))?;
        Ok(())
    }

    fn count_index_worktree_changes(&self) -> Option<usize> {
        let platform = self.handle.status(Discard).ok()?;
        let iter = platform.into_index_worktree_iter(Vec::new()).ok()?;
        Some(iter.filter(|item| item.is_ok()).count())
    }

    fn compute_tracking_status(&self) -> Result<Option<TrackingStatus>, RepoError> {
        let Some(head_full) = self
            .handle
            .head_name()
            .map_err(|e| RepoError::References(e.to_string()))?
        else {
            return Ok(None);
        };

        let tracking_ref = match self
            .handle
            .branch_remote_tracking_ref_name(head_full.as_ref(), gix::remote::Direction::Fetch)
        {
            Some(Ok(name)) => name.into_owned(),
            Some(Err(e)) => return Err(RepoError::References(e.to_string())),
            None => return Ok(None),
        };

        let local_id = self
            .handle
            .find_reference(head_full.as_ref())
            .map_err(|e| RepoError::References(e.to_string()))?
            .peel_to_id_in_place()
            .map_err(|e| RepoError::References(e.to_string()))?
            .detach();
        let upstream_id = match self.handle.find_reference(tracking_ref.as_ref()) {
            Ok(mut r) => r
                .peel_to_id_in_place()
                .map_err(|e| RepoError::References(e.to_string()))?
                .detach(),
            Err(_) => {
                // Upstream tracking ref configured but not yet fetched; report
                // tracking-known state with zeroed counts.
                return Ok(Some(TrackingStatus {
                    remote_branch: tracking_ref.as_bstr().to_str_lossy().into_owned(),
                    ahead: 0,
                    behind: 0,
                }));
            }
        };

        let (ahead, behind) = if local_id == upstream_id {
            (0, 0)
        } else {
            count_ahead_behind(&self.handle, local_id, upstream_id)?
        };

        Ok(Some(TrackingStatus {
            remote_branch: tracking_ref.as_bstr().to_str_lossy().into_owned(),
            ahead,
            behind,
        }))
    }
}

/// Compute `ahead` (commits reachable from `local` not from `upstream`) and
/// `behind` (commits reachable from `upstream` not from `local`).
///
/// Walks each side fully and uses set difference. For workspace-sized
/// histories (thousands of commits, not millions) the cost is acceptable;
/// huge monorepos will want a merge-base-driven path when gix exposes one.
fn count_ahead_behind(
    repo: &gix::Repository,
    local: gix::ObjectId,
    upstream: gix::ObjectId,
) -> Result<(usize, usize), RepoError> {
    let local_set = walk_to_set(repo, local)?;
    let upstream_set = walk_to_set(repo, upstream)?;
    let ahead = local_set.difference(&upstream_set).count();
    let behind = upstream_set.difference(&local_set).count();
    Ok((ahead, behind))
}

fn walk_to_set(
    repo: &gix::Repository,
    from: gix::ObjectId,
) -> Result<HashSet<gix::ObjectId>, RepoError> {
    let walk = repo
        .rev_walk(std::iter::once(from))
        .all()
        .map_err(|e| RepoError::References(e.to_string()))?;
    let mut set = HashSet::new();
    for item in walk {
        let info = item.map_err(|e| RepoError::References(e.to_string()))?;
        set.insert(info.id);
    }
    Ok(set)
}

fn write_config_to_disk(
    repo: &gix::Repository,
    file: &gix::config::File<'static>,
) -> Result<(), RepoError> {
    let path = repo.git_dir().join("config");
    let rendered = file.to_bstring();
    std::fs::write(&path, rendered.as_slice()).map_err(|source| RepoError::Io { path, source })
}
