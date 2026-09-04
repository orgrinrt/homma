//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Git as a subprocess, which is how the scripts this replaces reached it.
//!
//! The launcher's one dependency is renki and stays so; the engine has gix
//! and the engine is not here. Every question asked of git is one the
//! command line answers in a line or two, and every answer is read as text.

use std::path::Path;
use std::process::Command;

/// Run git with `args`, in `cwd` where given, and hand back its stdout.
///
/// A nonzero exit is an error carrying stderr, trimmed, so a refusal names
/// what git said rather than that git said something.
pub fn run(args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("git could not be run: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("git {} failed: {err}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Refuse by name when git is not on the path, ahead of the first clone,
/// since the alternative is a clone failing with a message about a
/// directory.
pub fn is_available() -> Result<(), String> {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|_| ())
        .map_err(|e| format!("git is not on PATH, and spawning needs it: {e}"))
}

/// Whether the remote at `url` has a branch of this name.
pub fn has_branch(url: &str, branch: &str) -> bool {
    Command::new("git")
        .args(["ls-remote", "--exit-code", "--heads", url, branch])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The working trunk of the repository at `url`: `dev` where it has one,
/// otherwise whatever its HEAD points at.
///
/// A plain clone takes the forge's default, which is the public face and
/// moves on a release, so a workspace cloned that way runs the rules as of
/// the last release rather than the ones being worked on.
pub fn trunk_of(url: &str) -> Result<String, String> {
    if has_branch(url, "dev") {
        return Ok("dev".into());
    }
    let out = run(&["ls-remote", "--symref", url, "HEAD"], None)?;
    out.lines()
        .find_map(|l| {
            l.strip_prefix("ref: refs/heads/")
                .and_then(|r| r.split_whitespace().next())
                .map(str::to_owned)
        })
        .ok_or_else(|| format!("{url} names no default branch, so there is nothing to clone"))
}

/// Clone `url` at `branch` into `dest`, quietly.
pub fn clone(url: &str, branch: &str, dest: &Path) -> Result<(), String> {
    let dest = dest
        .to_str()
        .ok_or_else(|| format!("{} is not text, and git takes text", dest.display()))?;
    run(&["clone", "--quiet", "--branch", branch, url, dest], None).map(|_| ())
}

/// Create `branch` in `dir` off whatever it is on, and switch to it.
pub fn switch_new(dir: &Path, branch: &str) -> Result<(), String> {
    run(&["switch", "--quiet", "-c", branch], Some(dir)).map(|_| ())
}

/// Whether `dir` is a clone: a `.git` directory, not a worktree's `.git`
/// file, which is what the manifest's own member rule counts too.
pub fn is_clone(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

/// The branch `dir` is on, or `HEAD` when detached.
pub fn current_branch(dir: &Path) -> Result<String, String> {
    let b = run(&["branch", "--show-current"], Some(dir))?;
    let b = b.trim();
    Ok(if b.is_empty() { "HEAD".into() } else { b.to_owned() })
}

/// The short hash of `dir`'s HEAD.
pub fn short_head(dir: &Path) -> Result<String, String> {
    run(&["rev-parse", "--short", "HEAD"], Some(dir)).map(|s| s.trim().to_owned())
}

/// The porcelain status of `dir`, one entry per line, empty when clean.
///
/// A nested clone shows here as an untracked directory, `?? name/`, unless
/// the repository ignores it, so a caller surveying a workspace takes the
/// member directories out before calling the rest dirt.
pub fn status_lines(dir: &Path) -> Result<Vec<String>, String> {
    run(&["status", "--porcelain"], Some(dir)).map(|s| s.lines().map(str::to_owned).collect())
}

/// Every commit reachable from a local branch of `dir` and from no remote
/// branch, one line each.
pub fn unpushed(dir: &Path) -> Result<Vec<String>, String> {
    run(
        &["log", "--branches", "--not", "--remotes", "--oneline"],
        Some(dir),
    )
    .map(|s| s.lines().map(str::to_owned).collect())
}
