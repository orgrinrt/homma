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
    let out = trimmed(cwd, &["ls-remote", "--tags", remote])?;
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
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        for args in [
            vec!["init", "--quiet", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "tag.gpgsign", "false"],
        ] {
            git(p, &args).unwrap();
        }
        std::fs::write(p.join("a"), "a").unwrap();
        commit_paths(p, &["a"], "feat: first").unwrap();
        d
    }

    #[test]
    fn a_clean_tree_is_clean_and_an_untracked_file_dirties_it() {
        let d = repo();
        assert!(is_clean(d.path()).unwrap());
        std::fs::write(d.path().join("b"), "b").unwrap();
        assert!(!is_clean(d.path()).unwrap());
    }

    #[test]
    fn the_branch_is_read_and_a_detached_head_is_none() {
        let d = repo();
        assert_eq!(current_branch(d.path()).unwrap().as_deref(), Some("main"));
        let h = head(d.path()).unwrap();
        git(d.path(), &["checkout", "--quiet", &h]).unwrap();
        assert_eq!(current_branch(d.path()).unwrap(), None);
    }

    #[test]
    fn an_annotated_tag_is_told_from_a_lightweight_one_and_points_at_its_commit() {
        let d = repo();
        let h = head(d.path()).unwrap();
        tag_annotated(d.path(), "v0.1.0", &h, "v0.1.0").unwrap();
        git(d.path(), &["tag", "light", &h]).unwrap();
        assert!(tag_is_annotated(d.path(), "v0.1.0").unwrap());
        assert!(!tag_is_annotated(d.path(), "light").unwrap());
        assert_eq!(tag_target(d.path(), "v0.1.0").unwrap(), h);
        let mut t = tags(d.path()).unwrap();
        t.sort();
        assert_eq!(t, vec!["light".to_string(), "v0.1.0".to_string()]);
        assert!(matches!(sha(d.path(), "nope"), Err(GitError::Missing(_))));
    }

    #[test]
    fn subjects_come_newest_first_with_the_pr_number_where_a_merge_carries_one() {
        let d = repo();
        let base = head(d.path()).unwrap();
        std::fs::write(d.path().join("b"), "b").unwrap();
        commit_paths(d.path(), &["b"], "fix: second").unwrap();
        std::fs::write(d.path().join("c"), "c").unwrap();
        commit_paths(d.path(), &["c"], "Merge pull request #7 from x/y").unwrap();
        std::fs::write(d.path().join("d"), "d").unwrap();
        commit_paths(d.path(), &["d"], "docs: rewrite readme (#9)").unwrap();
        let s = subjects(d.path(), &base, "HEAD").unwrap();
        let got: Vec<(&str, Option<u64>)> = s.iter().map(|x| (x.subject.as_str(), x.pr)).collect();
        assert_eq!(got, vec![
            ("docs: rewrite readme (#9)", Some(9)),
            ("Merge pull request #7 from x/y", Some(7)),
            ("fix: second", None),
        ]);
        assert_eq!(pr_number("fix: issue #12 in parser"), None);
    }

    #[test]
    fn a_no_ff_merge_makes_a_two_parent_commit_and_a_tag_lands_on_it() {
        let d = repo();
        let p = d.path();
        git(p, &["switch", "--quiet", "-c", "dev"]).unwrap();
        std::fs::write(p.join("b"), "b").unwrap();
        commit_paths(p, &["b"], "feat: on dev").unwrap();
        switch(p, "main").unwrap();
        let merge = merge_no_ff(p, "dev", "release: 0.1.0").unwrap();
        assert_eq!(parent_count(p, &merge).unwrap(), 2);
        assert!(is_ancestor(p, "dev", "main").unwrap());
        assert!(!is_ancestor(p, "main", "dev").unwrap());
        tag_annotated(p, "v0.1.0", &merge, "v0.1.0").unwrap();
        assert_eq!(tag_target(p, "v0.1.0").unwrap(), merge);
    }

    #[test]
    fn an_orphan_branch_holds_only_its_files_and_has_no_parent() {
        let d = repo();
        let p = d.path();
        let c = write_orphan_branch(
            p,
            "badges",
            &[("tests.json", "{\"a\":1}"), ("gate.json", "{}")],
            "badges",
        )
        .unwrap();
        assert_eq!(parent_count(p, &c).unwrap(), 0);
        let mut files = files_on(p, "badges").unwrap();
        files.sort();
        assert_eq!(files, vec![
            ("gate.json".to_string(), "{}".to_string()),
            ("tests.json".to_string(), "{\"a\":1}".to_string()),
        ]);
        assert!(is_clean(p).unwrap());
        assert_eq!(current_branch(p).unwrap().as_deref(), Some("main"));
        let again = write_orphan_branch(p, "badges", &[("gate.json", "x")], "badges").unwrap();
        assert_eq!(files_on(p, "badges").unwrap(), vec![(
            "gate.json".to_string(),
            "x".to_string()
        )]);
        assert_eq!(parent_count(p, &again).unwrap(), 0);
    }

    #[test]
    fn commit_paths_leaves_other_staged_files_alone() {
        let d = repo();
        let p = d.path();
        std::fs::write(p.join("mine"), "m").unwrap();
        std::fs::write(p.join("theirs"), "t").unwrap();
        git(p, &["add", "theirs"]).unwrap();
        commit_paths(p, &["mine"], "chore: mine").unwrap();
        let status = trimmed(p, &["status", "--porcelain"]).unwrap();
        assert_eq!(status, "A  theirs");
    }

    #[test]
    fn modified_and_untracked_are_told_apart_and_tracked_at_reads_a_rev() {
        let d = repo();
        let p = d.path();
        assert!(modified(p).unwrap().is_empty());
        assert!(untracked(p).unwrap().is_empty());
        std::fs::write(p.join("a"), "changed").unwrap();
        std::fs::create_dir(p.join("d")).unwrap();
        std::fs::write(p.join("d/new"), "n").unwrap();
        assert_eq!(modified(p).unwrap(), vec!["a"]);
        assert_eq!(untracked(p).unwrap(), vec!["d/new"]);
        assert_eq!(tracked_at(p, "HEAD").unwrap(), vec!["a"]);
        assert_eq!(show(p, "HEAD", "a").unwrap().as_deref(), Some("a"));
        assert_eq!(show(p, "HEAD", "d/new").unwrap(), None);
    }

    #[test]
    fn a_remote_answers_its_tags_peeled_and_a_branch_is_pushed_only_once_it_is_there() {
        let d = repo();
        let p = d.path();
        let bare = tempfile::tempdir().unwrap();
        git(bare.path(), &["init", "--quiet", "--bare"]).unwrap();
        git(p, &[
            "remote",
            "add",
            "origin",
            bare.path().to_str().unwrap(),
        ])
        .unwrap();
        assert!(!is_pushed(p, "origin", "main").unwrap());
        push(p, "origin", "main", false).unwrap();
        assert!(is_pushed(p, "origin", "main").unwrap());
        let h = head(p).unwrap();
        tag_annotated(p, "v0.1.0", &h, "v0.1.0").unwrap();
        git(p, &["tag", "light", &h]).unwrap();
        assert!(remote_tags(p, "origin").unwrap().is_empty());
        push(p, "origin", "refs/tags/v0.1.0", false).unwrap();
        push(p, "origin", "refs/tags/light", false).unwrap();
        let tags = remote_tags(p, "origin").unwrap();
        assert_eq!(tags, vec![
            ("light".to_string(), h.clone()),
            ("v0.1.0".to_string(), h.clone())
        ]);
        std::fs::write(p.join("b"), "b").unwrap();
        commit_paths(p, &["b"], "feat: b").unwrap();
        assert!(
            !is_pushed(p, "origin", "main").unwrap(),
            "a new commit is unpushed"
        );
        fetch(p, "origin").unwrap();
        assert!(!is_pushed(p, "origin", "main").unwrap());
    }
}
