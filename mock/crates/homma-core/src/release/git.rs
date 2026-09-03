//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The git operations a release needs and `RepoOps` does not carry: reading
//! history and tags, merging, tagging, pushing, committing named paths, and
//! writing an orphan branch. Each is a thin call to `git` through `sh`.

use std::fmt;
use std::path::Path;

use super::sh;

/// A git call that ran and refused, or could not run.
#[derive(Debug)]
pub enum GitError {
    /// `git` exited non-zero; the command line and its stderr.
    Failed {
        command: String,
        stderr:  String,
    },
    /// `git` could not be started.
    Spawn(sh::Spawn),
    /// A ref or an object that was asked for is not there.
    Missing(String),
    /// A scratch file a read needed could not be written, a planted path
    /// included.
    Scratch(std::io::Error),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::Failed {
                command,
                stderr,
            } => write!(f, "`{command}` failed: {}", stderr.trim()),
            GitError::Spawn(s) => write!(f, "{s}"),
            GitError::Missing(what) => write!(f, "{what} is not there"),
            GitError::Scratch(e) => write!(f, "a scratch file could not be written: {e}"),
        }
    }
}

impl std::error::Error for GitError {}

impl From<sh::Spawn> for GitError {
    fn from(s: sh::Spawn) -> Self {
        GitError::Spawn(s)
    }
}

/// One commit as the changelog sees it: its short sha, its subject, and the
/// pull request number a merge subject carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub sha:     String,
    pub subject: String,
    pub pr:      Option<u64>,
}

fn git(cwd: &Path, args: &[&str]) -> Result<sh::Output, GitError> {
    let out = sh::run(cwd, "git", args)?;
    if out.ok() {
        Ok(out)
    } else {
        Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        })
    }
}

fn trimmed(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    Ok(git(cwd, args)?.stdout.trim().to_string())
}

/// The full sha of `rev`.
pub fn sha(cwd: &Path, rev: &str) -> Result<String, GitError> {
    let s = trimmed(cwd, &[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{rev}^{{commit}}"),
    ])
    .map_err(|_| GitError::Missing(rev.to_string()))?;
    if s.is_empty() { Err(GitError::Missing(rev.to_string())) } else { Ok(s) }
}

/// The sha of `HEAD`.
pub fn head(cwd: &Path) -> Result<String, GitError> {
    sha(cwd, "HEAD")
}

/// The branch `HEAD` is on, or `None` when it is detached.
pub fn current_branch(cwd: &Path) -> Result<Option<String>, GitError> {
    let s = trimmed(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    match s {
        Ok(b) if !b.is_empty() => Ok(Some(b)),
        _ => Ok(None),
    }
}

/// Whether the tree has no change, staged or not, tracked or untracked.
pub fn is_clean(cwd: &Path) -> Result<bool, GitError> {
    Ok(trimmed(cwd, &["status", "--porcelain"])?.is_empty())
}

/// Every tag, oldest first by creation order is not something git keeps, so
/// this is name order; callers that need version order parse and sort.
pub fn tags(cwd: &Path) -> Result<Vec<String>, GitError> {
    let out = trimmed(cwd, &["tag", "--list"])?;
    Ok(out
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect())
}

/// The commit a tag points at, through the tag object when it is annotated.
pub fn tag_target(cwd: &Path, tag: &str) -> Result<String, GitError> {
    sha(cwd, tag)
}

/// Whether `tag` is an annotated tag rather than a lightweight one.
pub fn tag_is_annotated(cwd: &Path, tag: &str) -> Result<bool, GitError> {
    Ok(trimmed(cwd, &["cat-file", "-t", tag])? == "tag")
}

/// Whether `ancestor` is reachable from `descendant`.
pub fn is_ancestor(cwd: &Path, ancestor: &str, descendant: &str) -> Result<bool, GitError> {
    let out = sh::run(cwd, "git", &[
        "merge-base",
        "--is-ancestor",
        ancestor,
        descendant,
    ])?;
    match out.status {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            Err(GitError::Failed {
                command: out.command_line(),
                stderr:  out.stderr,
            })
        },
    }
}

/// The subjects of the commits in `from..to`, newest first, with the pull
/// request number when the subject is a merge of one.
pub fn subjects(cwd: &Path, from: &str, to: &str) -> Result<Vec<Subject>, GitError> {
    let range = format!("{from}..{to}");
    let out = trimmed(cwd, &["log", "--format=%h%x09%s", &range])?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (sha, subject) = line.split_once('\t').unwrap_or((line, ""));
            Subject {
                sha:     sha.to_string(),
                subject: subject.to_string(),
                pr:      pr_number(subject),
            }
        })
        .collect())
}

/// Every subject reachable from `to`, newest first, for a repo with no tag
/// yet, where the first release carries its whole history.
pub fn subjects_to(cwd: &Path, to: &str) -> Result<Vec<Subject>, GitError> {
    let out = trimmed(cwd, &["log", "--format=%h%x09%s", to])?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (sha, subject) = line.split_once('\t').unwrap_or((line, ""));
            Subject {
                sha:     sha.to_string(),
                subject: subject.to_string(),
                pr:      pr_number(subject),
            }
        })
        .collect())
}

/// The shas on `rev`'s first-parent walk whose diff against their first
/// parent touches `path`, newest first. A merge counts where the merge
/// brought the change in, which is how a bump made on the trunk shows on
/// the release line.
pub fn first_parent_touching(cwd: &Path, rev: &str, path: &str) -> Result<Vec<String>, GitError> {
    let out = trimmed(cwd, &[
        "log",
        "--first-parent",
        "--format=%H",
        rev,
        "--",
        path,
    ])?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// The number in `Merge pull request #12 from` or a trailing `(#12)`.
fn pr_number(subject: &str) -> Option<u64> {
    let idx = subject.find('#')?;
    let digits: String = subject[idx + 1 ..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    let before = &subject[.. idx];
    if before.contains("pull request") || before.ends_with('(') {
        digits.parse().ok()
    } else {
        None
    }
}

/// Merge `from` into the current branch with a merge commit, refusing a fast
/// forward so the tag has a commit of its own.
pub fn merge_no_ff(cwd: &Path, from: &str, message: &str) -> Result<String, GitError> {
    git(cwd, &["merge", "--no-ff", "--no-edit", "-m", message, from])?;
    head(cwd)
}

/// Abort a merge in progress. Fails where none is, which a caller cleaning up
/// after a failed step ignores.
/// Move the checked-out branch back to `rev`, discarding what sits past it.
/// Only ever run on a branch this tool moved itself, to undo that move.
pub fn reset_hard(cwd: &Path, rev: &str) -> Result<(), GitError> {
    git(cwd, &["reset", "--quiet", "--hard", rev]).map(|_| ())
}

pub fn abort_merge(cwd: &Path) -> Result<(), GitError> {
    git(cwd, &["merge", "--abort"]).map(|_| ())
}

/// Switch to `branch`.
pub fn switch(cwd: &Path, branch: &str) -> Result<(), GitError> {
    git(cwd, &["switch", "--quiet", branch]).map(|_| ())
}

/// An annotated tag `name` on `sha` with `message`.
pub fn tag_annotated(cwd: &Path, name: &str, sha: &str, message: &str) -> Result<(), GitError> {
    git(cwd, &["tag", "-a", name, sha, "-m", message]).map(|_| ())
}

/// Push one refspec to `remote`; `force` for the badges branch only.
pub fn push(cwd: &Path, remote: &str, refspec: &str, force: bool) -> Result<(), GitError> {
    if force {
        git(cwd, &["push", "--quiet", "--force", remote, refspec]).map(|_| ())
    } else {
        git(cwd, &["push", "--quiet", remote, refspec]).map(|_| ())
    }
}

/// Stage exactly `paths` and commit them, and nothing else that may be staged,
/// which is what naming the paths on the commit guarantees.
pub fn commit_paths(cwd: &Path, paths: &[&str], message: &str) -> Result<String, GitError> {
    let mut add = vec!["add", "--"];
    add.extend_from_slice(paths);
    git(cwd, &add)?;
    let mut commit = vec!["commit", "--quiet", "-m", message, "--"];
    commit.extend_from_slice(paths);
    git(cwd, &commit)?;
    head(cwd)
}

/// Write `files` as the whole content of the branch `name`, as one commit with
/// no parent, and point the branch at it. The tree is built from blobs
/// through the plumbing so the working tree is never touched.
pub fn write_orphan_branch(
    cwd: &Path,
    name: &str,
    files: &[(&str, &str)],
    message: &str,
) -> Result<String, GitError> {
    let mut entries = String::new();
    for (path, content) in files {
        let blob = sh::run_stdin(cwd, "git", &["hash-object", "-w", "--stdin"], content)?;
        if !blob.ok() {
            return Err(GitError::Failed {
                command: blob.command_line(),
                stderr:  blob.stderr,
            });
        }
        entries.push_str(&format!("100644 blob {}\t{}\n", blob.stdout.trim(), path));
    }
    let tree = sh::run_stdin(cwd, "git", &["mktree"], &entries)?;
    if !tree.ok() {
        return Err(GitError::Failed {
            command: tree.command_line(),
            stderr:  tree.stderr,
        });
    }
    let tree = tree.stdout.trim().to_string();
    let commit = trimmed(cwd, &["commit-tree", &tree, "-m", message])?;
    git(cwd, &["update-ref", &format!("refs/heads/{name}"), &commit])?;
    Ok(commit)
}

/// The files on `branch`, path and content, for reading a badges branch back.
pub fn files_on(cwd: &Path, branch: &str) -> Result<Vec<(String, String)>, GitError> {
    let listing = trimmed(cwd, &["ls-tree", "--name-only", "-r", branch])?;
    let mut out = Vec::new();
    for path in listing.lines().filter(|l| !l.is_empty()) {
        let content = git(cwd, &["show", &format!("{branch}:{path}")])?.stdout;
        out.push((path.to_string(), content));
    }
    Ok(out)
}

/// How many parents a commit has; an orphan has none and a merge has two.
pub fn parent_count(cwd: &Path, rev: &str) -> Result<usize, GitError> {
    let out = trimmed(cwd, &["rev-list", "--parents", "-n", "1", rev])?;
    Ok(out.split_whitespace().count().saturating_sub(1))
}

/// The tags on `remote` with the commit each points at, peeled through the
/// tag object, so an annotated tag reports its commit the way `tag_target`
/// does locally.
pub fn remote_tags(cwd: &Path, remote: &str) -> Result<Vec<(String, String)>, GitError> {
    // a remote that stops answering is given up on in the same order of time
    // the registry client allows, rather than for as long as the network
    // takes; git has no wall bound, so this is the bound it does have
    let out = sh::run_with_env(cwd, "git", &["ls-remote", "--tags", remote], &[
        ("GIT_HTTP_LOW_SPEED_LIMIT", "1000"),
        ("GIT_HTTP_LOW_SPEED_TIME", "15"),
    ])?;
    if !out.ok() {
        return Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        });
    }
    let out = out.stdout.trim().to_string();
    let mut peeled: Vec<(String, String)> = Vec::new();
    let mut plain: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        let Some((sha, r)) = line.split_once('\t') else { continue };
        let Some(name) = r.strip_prefix("refs/tags/") else { continue };
        match name.strip_suffix("^{}") {
            Some(n) => peeled.push((n.to_string(), sha.to_string())),
            None => plain.push((name.to_string(), sha.to_string())),
        }
    }
    for (name, sha) in plain {
        if !peeled.iter().any(|(n, _)| *n == name) {
            peeled.push((name, sha));
        }
    }
    peeled.sort();
    Ok(peeled)
}

/// Whether `branch` on `remote` holds every commit of the local `branch`.
/// A branch with no remote counterpart is unpushed by definition.
pub fn is_pushed(cwd: &Path, remote: &str, branch: &str) -> Result<bool, GitError> {
    let upstream = format!("{remote}/{branch}");
    match sha(cwd, &upstream) {
        Ok(_) => is_ancestor(cwd, branch, &upstream),
        Err(GitError::Missing(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// The paths `git status` reports as untracked.
pub fn untracked(cwd: &Path) -> Result<Vec<String>, GitError> {
    // not trimmed: the two status columns may open with a space
    let out = git(cwd, &["status", "--porcelain", "--untracked-files=all"])?.stdout;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix("?? "))
        .map(str::to_string)
        .collect())
}

/// The paths `git status` reports as modified, staged or not, tracked only.
pub fn modified(cwd: &Path) -> Result<Vec<String>, GitError> {
    let out = git(cwd, &["status", "--porcelain"])?.stdout;
    Ok(out
        .lines()
        .filter(|l| !l.starts_with("?? ") && l.len() > 3)
        .map(|l| l[3 ..].to_string())
        .collect())
}

/// Every tracked path at `rev`.
pub fn tracked_at(cwd: &Path, rev: &str) -> Result<Vec<String>, GitError> {
    let out = trimmed(cwd, &["ls-tree", "-r", "--name-only", rev])?;
    Ok(out
        .lines()
        .map(str::to_string)
        .filter(|l| !l.is_empty())
        .collect())
}

/// The content of `path` at `rev`, or none when it is not there.
pub fn show(cwd: &Path, rev: &str, path: &str) -> Result<Option<String>, GitError> {
    let out = sh::run(cwd, "git", &["show", &format!("{rev}:{path}")])?;
    Ok(out.ok().then_some(out.stdout))
}

/// Fetch tags and branches from `remote`, pruning what is gone.
pub fn fetch(cwd: &Path, remote: &str) -> Result<(), GitError> {
    git(cwd, &["fetch", "--quiet", "--tags", "--prune", remote]).map(|_| ())
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
