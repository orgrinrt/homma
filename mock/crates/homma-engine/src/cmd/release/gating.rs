//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! `homma release gate`: the gate on the checkout, or on every tip a
//! pre-push hook is handed, each run recorded and its status posted; from
//! the hook, posted by a poster left behind, since the forge has not
//! received the commit while the hook runs.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use homma_api::Verdict;
use homma_core::Config;
use homma_core::forge::ForgeError;
use homma_core::release::gate::{self, Real};
use homma_core::release::{git, status};

use super::{Outcome, Report, clock, finish, forge_for, record, resolve_repo, store};
use crate::cli::Cli;

/// How long a poster asks after the commit before giving up, and how often.
const AWAIT: Duration = Duration::from_secs(600);
const PAUSE: Duration = Duration::from_secs(5);

pub(super) fn gate_cmd(
    cli: &Cli,
    cfg: &Config,
    repo: Option<&str>,
    sha: Option<&str>,
    hook: bool,
    post: Option<&str>,
    wait: bool,
) -> Result<Outcome> {
    let (name, r, root) = resolve_repo(cfg, repo)?;
    let root = &root;
    let store = store(cli);
    let (forge, owner) = forge_for(cfg, r)?;
    if let Some(want) = post {
        // records carry the full sha, so what was typed is resolved first,
        // the same way `--sha` is
        let sha = &git::sha(root, want)
            .map_err(|_| anyhow!("`{want}` is not a commit in this repository"))?;
        let run = record::newest_for(&store, name, sha)?
            .ok_or_else(|| anyhow!("no gate run recorded on {sha}"))?;
        if wait {
            let tries = (AWAIT.as_secs() / PAUSE.as_secs()) as usize;
            let known = await_known(|| forge.commit_known(&owner, name, sha), tries, PAUSE)?;
            if !known {
                return finish(cli, Report {
                    ok:    false,
                    lines: vec![format!(
                        "the forge still does not know {sha} after {} seconds; the record is kept and `homma release gate --post {}` posts it",
                        AWAIT.as_secs(),
                        &sha[.. 7]
                    )],
                });
            }
        }
        status::post(forge.as_ref(), &owner, name, &run)
            .with_context(|| format!("posting the status on {sha}"))?;
        return finish(cli, Report {
            ok:    true,
            lines: vec![format!(
                "posted {} on {sha}: {}",
                status::CONTEXT,
                status::description(&run)
            )],
        });
    }
    let head = homma_core::release::git::head(root)?;
    // the hook gates every tip being pushed, the head in place and any other
    // in a worktree of its own; a plain run gates the checkout
    let tips: Vec<String> = if hook {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
        let tips = peeled(root, &pushed_tips(&text))?;
        if tips.is_empty() {
            return finish(cli, Report {
                ok:    true,
                lines: vec!["no ref is being pushed; nothing to gate".into()],
            });
        }
        tips
    } else {
        // a commit named by hand is gated the way a pushed tip is: the head
        // in place, any other in a worktree of its own
        match sha {
            Some(want) => {
                let full = homma_core::release::git::sha(root, want)
                    .map_err(|_| anyhow!("`{want}` is not a commit in this repository"))?;
                vec![full]
            },
            None => vec![head],
        }
    };
    let mut lines = Vec::new();
    let mut ok = true;
    for tip in &tips {
        let run = gate::run_gate_at(&Real, root, &cfg.markers, tip, name, &clock::now())?;
        record::append(&store, &run).context("recording the run")?;
        lines.push(run.summary());
        ok &= run.verdict == Verdict::Green;
        if hook {
            // the forge has not received this commit yet, and will not until
            // the hook returns; a red run refuses the push and there is
            // nothing to tell the forge about
            if run.verdict == Verdict::Green {
                match leave_poster(name, &run.sha) {
                    Ok(()) => lines.push(format!(
                        "a poster will put {} on {} once the push has landed",
                        status::CONTEXT,
                        &run.sha[.. 7]
                    )),
                    Err(e) => lines.push(format!(
                        "no poster could be left ({e}); the record is kept and `homma release gate --post {}` posts it",
                        &run.sha[.. 7]
                    )),
                }
            }
            continue;
        }
        match status::post(forge.as_ref(), &owner, name, &run) {
            Ok(()) => lines.push(format!("posted {} on {}", status::CONTEXT, &run.sha[.. 7])),
            Err(e) => {
                lines.push(format!(
                    "the status was not posted ({e}); the record is kept and `homma release gate --post {}` posts it",
                    &run.sha[.. 7]
                ))
            },
        }
    }
    finish(cli, Report {
        ok,
        lines,
    })
}

/// The tips as commits, once each: an annotated tag's line carries the tag
/// object's sha, and a `--follow-tags` push hands the branch and the tag on
/// one commit as two lines.
fn peeled(root: &std::path::Path, tips: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for tip in tips {
        let commit = git::sha(root, tip)
            .map_err(|_| anyhow!("`{tip}` is being pushed and is not a commit or a tag on one"))?;
        if !out.contains(&commit) {
            out.push(commit);
        }
    }
    Ok(out)
}

/// Ask `known` until it answers yes, `tries` times with `pause` between, and
/// say whether it ever did. An error from the forge ends the wait, since it
/// is not evidence either way.
fn await_known(
    mut known: impl FnMut() -> Result<bool, ForgeError>,
    tries: usize,
    pause: Duration,
) -> Result<bool> {
    for i in 0 .. tries {
        if known().context("asking the forge after the commit")? {
            return Ok(true);
        }
        if i + 1 < tries {
            std::thread::sleep(pause);
        }
    }
    Ok(false)
}

/// The command a poster runs: this binary, on the repository by name, posting
/// the recorded run once the forge knows the commit.
fn poster_args(name: &str, sha: &str) -> Vec<String> {
    vec![
        "release".into(),
        "gate".into(),
        name.into(),
        "--post".into(),
        sha.into(),
        "--await".into(),
    ]
}

/// Leave a poster behind: this binary again, detached from the hook and from
/// git, with nothing on its streams, so git's own wait on the hook ends and
/// the push goes ahead while the poster asks after the commit.
fn leave_poster(name: &str, sha: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(poster_args(name, sha))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // its own process group, so the hook's exit does not take it along
        cmd.process_group(0);
    }
    cmd.spawn().map(drop)
}

/// The distinct local shas a pre-push hook is handed on stdin, one
/// `<local ref> <local sha> <remote ref> <remote sha>` per line, skipping a
/// deletion, whose local sha is all zeros.
fn pushed_tips(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sha in text.lines().filter_map(|l| l.split_whitespace().nth(1)) {
        if sha.chars().all(|c| c == '0') || out.iter().any(|s| s == sha) {
            continue;
        }
        out.push(sha.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_tag_and_the_branch_it_sits_on_are_one_commit_and_a_stranger_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(d.path())
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        g(&["init", "-q", "-b", "main"]);
        std::fs::write(d.path().join("a"), "a").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "one"]);
        let commit = g(&["rev-parse", "HEAD"]);
        g(&["tag", "-a", "-m", "the end", "round/end"]);
        let tag = g(&["rev-parse", "round/end"]);
        assert_ne!(
            tag, commit,
            "the control: an annotated tag is its own object"
        );
        assert_eq!(
            peeled(d.path(), &[commit.clone(), tag.clone()]).unwrap(),
            vec![commit.clone()]
        );
        assert_eq!(peeled(d.path(), &[tag]).unwrap(), vec![commit]);
        assert!(
            peeled(d.path(), &[
                "0000000000000000000000000000000000000000".into()
            ])
            .is_err()
        );
    }

    #[test]
    fn the_wait_ends_when_the_forge_knows_the_commit_or_the_tries_run_out() {
        let asked = Cell::new(0usize);
        let known = || {
            asked.set(asked.get() + 1);
            Ok(asked.get() >= 3)
        };
        assert!(await_known(known, 5, Duration::ZERO).unwrap());
        assert_eq!(asked.get(), 3, "it stops asking once it knows");
        let asked = Cell::new(0usize);
        let never = || {
            asked.set(asked.get() + 1);
            Ok(false)
        };
        assert!(!await_known(never, 4, Duration::ZERO).unwrap());
        assert_eq!(asked.get(), 4);
        // an error ends the wait rather than counting as a no
        let broken = || {
            Err(ForgeError::Unauthorized {
                reason: "no".into(),
            })
        };
        assert!(await_known(broken, 4, Duration::ZERO).is_err());
    }

    #[test]
    fn the_poster_is_this_binary_posting_the_run_by_repository_name_once_known() {
        assert_eq!(poster_args("notko", "abc123"), vec![
            "release", "gate", "notko", "--post", "abc123", "--await"
        ]);
    }

    #[test]
    fn the_tips_are_the_distinct_local_shas_and_a_deletion_is_not_one() {
        let text = "refs/heads/dev aaa1 refs/heads/dev bbb1\n\
                    refs/heads/topic ccc1 refs/heads/topic 0000\n\
                    refs/heads/again aaa1 refs/heads/again bbb1\n\
                    refs/heads/gone 0000000000000000000000000000000000000000 refs/heads/gone ddd1\n";
        assert_eq!(pushed_tips(text), vec![
            "aaa1".to_string(),
            "ccc1".to_string()
        ]);
        assert!(pushed_tips("").is_empty());
    }
}
