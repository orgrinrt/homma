//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What an entrypoint calls: work out what the event touched, run the entries
//! whose globs match in order with git's arguments appended and stdin replayed,
//! and stop at the first refusal, which becomes the hook's own.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use homma_api::Hooks;

use super::install::HookError;
use crate::release::git::GitError;
use crate::release::sh;

fn git(root: &Path, args: &[&str]) -> Result<sh::Output, HookError> {
    let out = sh::run(root, "git", args).map_err(GitError::from)?;
    if !out.ok() {
        return Err(GitError::Failed {
            command: out.command_line(),
            stderr:  out.stderr,
        }
        .into());
    }
    Ok(out)
}

fn lines(out: &sh::Output) -> Vec<String> {
    out.stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

const ZERO: &str = "0000000000000000000000000000000000000000";

/// The paths an event touches, in the order git lists them and each once.
///
/// The staged paths for the commit-side events; for `pre-push`, the union
/// over every ref on `stdin` of what its local tip adds over its remote,
/// widened to everything the tip carries where the remote is a new branch or
/// a commit this clone does not have. Any other event touches nothing, so an
/// entry with globs never runs for it and an entry without runs as always.
pub fn touched(root: &Path, event: &str, stdin: &str) -> Result<Vec<String>, HookError> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |paths: Vec<String>| {
        for p in paths {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    };
    match event {
        // the two the design names; `post-commit` would read an empty index
        // and `pre-merge-commit` is not a commit anybody staged
        "pre-commit" | "commit-msg" => {
            push(lines(&git(root, &["diff", "--cached", "--name-only"])?));
        },
        "pre-push" => {
            for line in stdin.lines() {
                let mut parts = line.split_whitespace();
                let (Some(_), Some(local), Some(_), Some(remote)) =
                    (parts.next(), parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                if local == ZERO {
                    continue;
                }
                let known = remote != ZERO
                    && sh::run(root, "git", &["cat-file", "-e", remote])
                        .map(|o| o.ok())
                        .unwrap_or(false);
                if known {
                    let range = format!("{remote}..{local}");
                    push(lines(&git(root, &["diff", "--name-only", &range])?));
                } else {
                    push(lines(&git(root, &["ls-tree", "-r", "--name-only", local])?));
                }
            }
        },
        _ => {},
    }
    Ok(out)
}

/// What one run of an event's entries came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub lines: Vec<String>,
    pub ok:    bool,
}

/// Run the event's entries against what it touched, in order, stopping at the
/// first non-zero exit. Each entry's own output goes straight to the terminal,
/// the way a hook's does; the lines here say what ran and how it went. Git's
/// arguments reach an entry through `{args}` and stdin is replayed to every
/// entry, since a hook that reads its refs would otherwise starve the next.
pub fn run(
    root: &Path,
    hooks: &Hooks,
    event: &str,
    args: &[String],
    stdin: &str,
) -> Result<Ran, HookError> {
    let entries = hooks.entries(event);
    if entries.is_empty() {
        return Ok(Ran {
            lines: vec![format!("{event}: no entries, nothing to run")],
            ok:    true,
        });
    }
    let touched = touched(root, event, stdin)?;
    let mut lines = Vec::new();
    for entry in entries {
        if !entry.runs_for(&touched) {
            lines.push(format!(
                "{event}: skipped `{}`, no path matches",
                entry.run()
            ));
            continue;
        }
        let matched = entry.matching(&touched);
        let command = entry.command(&matched, args);
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        if let Some(mut pipe) = child.stdin.take() {
            // a hook that never reads its stdin closes the pipe first, which
            // is not a failure of the entry
            let _ = pipe.write_all(stdin.as_bytes());
        }
        let status = child.wait()?;
        if status.success() {
            lines.push(format!("{event}: ran `{}`", entry.run()));
        } else {
            lines.push(format!(
                "{event}: refused by `{}`, exit {}",
                entry.run(),
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            ));
            return Ok(Ran {
                lines,
                ok: false,
            });
        }
    }
    Ok(Ran {
        lines,
        ok: true,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use homma_api::HookEntry;

    use super::*;

    fn git_ok(root: &Path, args: &[&str]) {
        let out = sh::run(
            root,
            "git",
            &[&["-c", "user.name=t", "-c", "user.email=t@t"][..], args].concat(),
        )
        .unwrap();
        assert!(out.ok(), "{}", out.log());
    }

    /// A repo with one commit of `a.md` and `b.rs`, then `c.md` and `d.rs`
    /// staged.
    fn repo() -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().unwrap();
        git_ok(d.path(), &["init", "--quiet", "-b", "main"]);
        for f in ["a.md", "b.rs"] {
            std::fs::write(d.path().join(f), "x").unwrap();
        }
        git_ok(d.path(), &["add", "."]);
        git_ok(d.path(), &["commit", "-qm", "one"]);
        let first = sh::run(d.path(), "git", &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        for f in ["c.md", "d.rs"] {
            std::fs::write(d.path().join(f), "y").unwrap();
        }
        git_ok(d.path(), &["add", "."]);
        (d, first)
    }

    fn table(event: &str, entries: Vec<(&str, &[&str])>) -> Hooks {
        let mut declared = BTreeMap::new();
        declared.insert(
            event.to_string(),
            entries
                .into_iter()
                .map(|(run, paths)| {
                    HookEntry::new(run, paths.iter().map(|s| s.to_string()).collect()).unwrap()
                })
                .collect(),
        );
        Hooks::new(declared).unwrap()
    }

    #[test]
    fn a_commit_touches_what_is_staged_and_a_push_what_the_tip_adds() {
        let (d, first) = repo();
        assert_eq!(touched(d.path(), "pre-commit", "").unwrap(), vec![
            "c.md", "d.rs"
        ]);
        assert_eq!(touched(d.path(), "commit-msg", "").unwrap(), vec![
            "c.md", "d.rs"
        ]);
        git_ok(d.path(), &["commit", "-qm", "two"]);
        let second = sh::run(d.path(), "git", &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_string();
        // an existing branch: what the tip adds over the remote
        let known = format!("refs/heads/main {second} refs/heads/main {first}\n");
        assert_eq!(touched(d.path(), "pre-push", &known).unwrap(), vec![
            "c.md", "d.rs"
        ]);
        // a new branch, or a remote this clone does not have: everything the tip carries
        let fresh = format!("refs/heads/main {second} refs/heads/main {ZERO}\n");
        assert_eq!(touched(d.path(), "pre-push", &fresh).unwrap(), vec![
            "a.md", "b.rs", "c.md", "d.rs"
        ]);
        let unknown = format!(
            "refs/heads/main {second} refs/heads/main {}\n",
            "f".repeat(40)
        );
        assert_eq!(touched(d.path(), "pre-push", &unknown).unwrap().len(), 4);
        // a deletion touches nothing, and two refs are one list
        let two = format!("refs/heads/gone {ZERO} refs/heads/gone {first}\n{known}{known}");
        assert_eq!(touched(d.path(), "pre-push", &two).unwrap(), vec![
            "c.md", "d.rs"
        ]);
        assert!(touched(d.path(), "pre-push", "").unwrap().is_empty());
        // an event the design does not name touches nothing, `post-commit`
        // among them, since after the commit the index is empty and a glob
        // entry there would never fire while saying "no path matches"
        for other in ["post-checkout", "post-commit", "pre-merge-commit"] {
            assert!(touched(d.path(), other, "").unwrap().is_empty(), "{other}");
        }
    }

    #[test]
    fn entries_run_in_order_on_what_matched_with_the_arguments_and_the_stdin() {
        let (d, _) = repo();
        let log = d.path().join("log");
        let h = table("pre-commit", vec![
            (
                &format!(
                    "printf 'md:%s ' {{paths}} >> {}; echo >> {}",
                    log.display(),
                    log.display()
                ),
                &["*.md"][..],
            ),
            (&format!("echo args:{{args}} >> {}", log.display()), &[][..]),
            // no placeholder, so git's arguments do not reach it, which is
            // what lets a plain command sit in the table at all
            (&format!("cat >> {}", log.display()), &[][..]),
            (&format!("echo never >> {}", log.display()), &["*.toml"][..]),
        ]);
        let ran = run(
            d.path(),
            &h,
            "pre-commit",
            &["one".into(), "t wo".into()],
            "the refs\n",
        )
        .unwrap();
        assert!(ran.ok, "{:?}", ran.lines);
        let got = std::fs::read_to_string(&log).unwrap();
        assert_eq!(got, "md:c.md \nargs:one t wo\nthe refs\n");
        assert_eq!(ran.lines.len(), 4);
        assert!(ran.lines[0].starts_with("pre-commit: ran `printf"));
        assert!(ran.lines[3].contains("skipped") && ran.lines[3].contains("no path matches"));
    }

    #[test]
    fn the_first_refusal_stops_the_rest_and_is_the_hooks_own() {
        let (d, _) = repo();
        let log = d.path().join("log");
        let h = table("pre-commit", vec![
            ("exit 3", &[][..]),
            (&format!("echo after >> {}", log.display()), &[][..]),
        ]);
        let ran = run(d.path(), &h, "pre-commit", &[], "").unwrap();
        assert!(!ran.ok);
        assert_eq!(ran.lines, vec!["pre-commit: refused by `exit 3`, exit 3"]);
        assert!(!log.exists(), "nothing ran past the refusal");
    }

    #[test]
    fn an_event_with_no_entries_passes_and_says_so() {
        let (d, _) = repo();
        let ran = run(d.path(), Hooks::defaults(), "commit-msg", &[], "").unwrap();
        assert!(ran.ok);
        assert_eq!(ran.lines, vec!["commit-msg: no entries, nothing to run"]);
    }
}
