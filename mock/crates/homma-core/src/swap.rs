//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The swap: one file written onto a remote branch as a compare and swap.
//!
//! The commit is built on a tip the caller read, through the plumbing and a
//! scratch index, so no working tree is read or touched, and pushed without
//! force. The remote takes it only while the branch is still at that tip, so a
//! refused push says somebody else's write landed first, and the caller reads
//! again and decides again. This is what a lease on the workspace trunk is
//! taken with: two sessions can never both land a write over one read.

use std::path::Path;

use crate::release::git::{self, GitError};
use crate::release::sh;

/// What became of a swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swapped {
    /// The remote branch now points at this commit.
    Landed(String),
    /// The branch moved past the tip the write was built on; nothing landed.
    Moved,
}

/// Write `content` to `path`, or remove `path` where `content` is none, as one
/// commit on `base` with `message`, and push it to `remote`'s `branch` without
/// force.
///
/// `base` is the tip the caller read the file at, normally what
/// [`git::fetch_branch`] answered. A landed commit also moves the clone's
/// tracking ref, so a read straight after sees it.
pub fn swap_file(
    cwd: &Path,
    remote: &str,
    branch: &str,
    base: &str,
    path: &str,
    content: Option<&str>,
    message: &str,
) -> Result<Swapped, GitError> {
    let tree = tree_with(cwd, base, path, content)?;
    let commit = git::trimmed(cwd, &["commit-tree", &tree, "-p", base, "-m", message])?;
    let refspec = format!("{commit}:refs/heads/{branch}");
    let out = sh::run(cwd, "git", &["push", "--quiet", remote, &refspec])?;
    if !out.ok() {
        if refused_as_moved(&out.stderr) {
            return Ok(Swapped::Moved);
        }
        return Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        });
    }
    // The tracking ref is a convenience for a read straight after; the write has
    // landed whatever becomes of it, and the next fetch sets it anyway.
    let _ = git::trimmed(cwd, &[
        "update-ref",
        &format!("refs/remotes/{remote}/{branch}"),
        &commit,
    ]);
    Ok(Swapped::Landed(commit))
}

/// Whether a refused push says the branch is not where the write was built.
fn refused_as_moved(stderr: &str) -> bool {
    ["[rejected]", "non-fast-forward", "fetch first"]
        .iter()
        .any(|m| stderr.contains(m))
}

/// The tree of `base` with `path` set to `content`, or without it, built in a
/// scratch index under the clone's git directory.
fn tree_with(
    cwd: &Path,
    base: &str,
    path: &str,
    content: Option<&str>,
) -> Result<String, GitError> {
    let name = format!("homma-swap-index-{}", std::process::id());
    let index = git::trimmed(cwd, &["rev-parse", "--git-path", &name])?;
    let index = cwd.join(index);
    let index_str = index.to_string_lossy().into_owned();
    let env = [("GIT_INDEX_FILE", index_str.as_str())];
    let result = (|| {
        indexed(cwd, &["read-tree", base], &env)?;
        match content {
            Some(text) => {
                let blob = sh::run_stdin(cwd, "git", &["hash-object", "-w", "--stdin"], text)?;
                if !blob.ok() {
                    return Err(GitError::Failed {
                        command: blob.command_line(),
                        stderr:  blob.stderr,
                    });
                }
                let info = format!("100644,{},{path}", blob.stdout.trim());
                indexed(cwd, &["update-index", "--add", "--cacheinfo", &info], &env)?;
            },
            None => {
                indexed(cwd, &["update-index", "--force-remove", "--", path], &env)?;
            },
        }
        Ok(indexed(cwd, &["write-tree"], &env)?
            .stdout
            .trim()
            .to_string())
    })();
    // A scratch index left behind is only a stale file in the git directory,
    // so a failure to remove it is not the swap's failure.
    let _ = std::fs::remove_file(&index);
    result
}

fn indexed(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<sh::Output, GitError> {
    let out = sh::run_with_env(cwd, "git", args, env)?;
    if out.ok() {
        Ok(out)
    } else {
        Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        })
    }
}

#[cfg(test)]
mod tests;
