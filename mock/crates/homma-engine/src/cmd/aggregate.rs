//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Workspace-level aggregation of per-repo mockspace agent hooks.
//!
//! For each member repo with rendered hooks under `.claude/hooks/`, this
//! module reads every event and matcher the repo's `settings.json` registers
//! each hook under, and makes each registration a row of the manifest homma
//! writes for claude-shook: the hook run by `{root}`, narrowed to the repo by
//! `paths`. The front reads where a call lands and runs only the rows it lands
//! in; the substantive logic still lives in the repo.
//!
//! Older homma wrote a wrapper per hook and a registration per wrapper. Those
//! are swept once the front has registered the rows, by
//! [`retire_wrappers`] and [`merge_settings`].

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use homma_api::{ContainedPath, Root};

pub(crate) use crate::cmd::agent_shook::Row;
use crate::cmd::agent_shook::{self, ROOT};

/// The words every file homma writes under `.claude/hooks/` carries in a
/// comment near its top, and the only thing that makes such a file homma's. A
/// name decides nothing, since a hand-written hook can carry any name.
pub(crate) const MANAGED_MARK: &str = "by `homma agent regen`";

/// Whether the file at `path` carries [`MANAGED_MARK`] in a comment within its
/// first five lines. A file that cannot be read as text does not.
pub(crate) fn carries_the_mark(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|s| {
        s.lines()
            .take(5)
            .any(|l| l.starts_with('#') && l.contains(MANAGED_MARK))
    })
}

/// Whether the host could run the file: a regular file with an execute bit on
/// unix, and any regular file elsewhere, where there is no bit to read.
pub(crate) fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// What one repository's pass carried and what it could not.
#[derive(Debug, Default)]
pub(crate) struct Aggregated {
    /// Hooks carried.
    pub hooks:    usize,
    /// Hooks not carried, each naming its path and why. Any of these fails the
    /// regen, since each is a hook no session opened at the root will run.
    pub problems: Vec<String>,
}

/// Aggregate one repo's hooks as rows, returning how many hooks were carried
/// and which were not.
///
/// Rules are no longer aggregated; per-repo rules auto-load from the
/// repo's `.claude/rules/` when Claude Code is opened with cwd inside
/// the repo. Any previously-aggregated rules at
/// `<workspace>/.claude/rules/<repo>--*.md` get cleaned on every regen
/// so upgrades from older homma versions converge automatically.
///
/// `rows` accumulates the rows the regen writes into the manifest after the
/// per-repo loop completes. Nothing under `.claude/hooks/` is written or
/// removed here: an older homma's wrappers go only once the front has
/// registered what replaces them.
pub(crate) fn aggregate_repo(
    root: &Root,
    repo_name: &str,
    repo_abs_path: &Path,
    rows: &mut Vec<Row>,
) -> Result<Aggregated> {
    let claude_dir = repo_abs_path.join(".claude");
    if !claude_dir.is_dir() {
        return Err(anyhow!(
            "repo `{repo_name}` has no .claude/ at {}; run `cargo mock` in the repo first",
            claude_dir.display(),
        ));
    }

    // Proven, as every write and removal of this pass is: a symlink one
    // component down would otherwise carry the removal into somebody else's
    // directory.
    let ws_rules = contain(root, ".claude/rules")?;
    root.create_dir_all(&ws_rules).ok();
    clean_stale(root, &ws_rules, repo_name, ".md")?;

    // The workspace-relative path is the portable half, and the only half a
    // row may carry. A repo declared outside the workspace has none, and there
    // the absolute path is the only shape available.
    let repo_rel = repo_abs_path
        .strip_prefix(root.as_abs())
        .unwrap_or(repo_abs_path);
    // Through `relative_str`, so a manifest writing `./acre` does not put a
    // `.` in the middle of the path the front compares against.
    let repo_rel = crate::cmd::util::relative_str(repo_rel);

    let mut out = Aggregated::default();
    out.hooks = aggregate_hooks(&claude_dir, repo_name, &repo_rel, rows, &mut out.problems)?;
    Ok(out)
}

/// Prove a workspace-relative path, naming what escaped when it does not.
fn contain(root: &Root, tail: &str) -> Result<ContainedPath> {
    root.contain(&root.as_abs().join(tail))
        .map_err(|e| anyhow!("{e}"))
}

/// Remove the retired rule copies for `prefix`, which predate the mark and
/// which nothing but homma ever named that way.
fn clean_stale(root: &Root, dir: &ContainedPath, prefix: &str, ext: &str) -> Result<()> {
    if !dir.as_path().is_dir() {
        return Ok(());
    }
    let prefix = format!("{prefix}--");
    for entry in
        fs::read_dir(dir.as_path()).with_context(|| format!("read {}", dir.as_path().display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.starts_with(&prefix) && s.ends_with(ext) {
            // Proven before removing. A removal's damage is done at the call,
            // and this loop reads a directory that a symlink may have made
            // somebody else's.
            let target = root.contain_under(dir, &name).map_err(|e| anyhow!("{e}"))?;
            root.remove_file(&target).ok();
        }
    }
    Ok(())
}

/// Remove every wrapper an older homma wrote under the workspace's
/// `.claude/hooks/`: every file named `<a>--<b>` there carrying
/// [`MANAGED_MARK`], `keep` aside. One that does not carry it is somebody's,
/// and stays.
pub(crate) fn retire_wrappers(root: &Root, keep: &str) -> Result<()> {
    let dir = contain(root, ".claude/hooks")?;
    if !dir.as_path().is_dir() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir.as_path()).with_context(|| format!("read {}", dir.as_path().display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s == keep || !s.contains("--") {
            continue;
        }
        let target = root
            .contain_under(&dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if carries_the_mark(target.as_path()) {
            root.remove_file(&target).ok();
        }
    }
    Ok(())
}

/// Characters that make a program or its arguments a line of shell rather
/// than words.
const SHELL: [char; 10] = ['&', ';', '|', '(', ')', '`', '<', '>', '"', '\''];

/// The words a registration runs its hook with, or the problem to report for
/// `shown` where they are a line of shell.
///
/// A row is an argument list, so an assignment in front of the hook fails
/// there and a separator before it runs something else: `cd dir &&
/// .claude/hooks/x.sh` would run a `cd` binary, which exits 0, and that would
/// read as the hook's approval.
pub(crate) fn program_words(runner: &str, shown: &str) -> Result<Vec<String>, String> {
    let assignment = runner
        .split_whitespace()
        .next()
        .is_some_and(|w| w.contains('='));
    if assignment || runner.contains(SHELL) || runner.contains("$(") || runner.contains('$') {
        return Err(format!(
            "`{shown}` is run through `{runner}`, which is a line of shell rather than a \
             program, and a row can only run a program with arguments; not carried"
        ));
    }
    Ok(runner.split_whitespace().map(str::to_string).collect())
}

/// The words after a registration's hook, or the problem to report for
/// `shown` where split on whitespace they would not be what the host was
/// handed.
pub(crate) fn argument_words(args: &str, shown: &str) -> Result<Vec<String>, String> {
    if args.contains(SHELL) || args.contains('$') || args.contains('\\') {
        return Err(format!(
            "`{shown}` is called with `{args}`, which split into words would not be what the \
             host was handed; not carried"
        ));
    }
    Ok(args.split_whitespace().map(str::to_string).collect())
}

/// Walk the repository's `.claude/hooks/` and add a row for every event and
/// matcher each hook is called under. A file the host could not run, one
/// nothing registers, and one run through a line of shell are each reported
/// in `problems` and carried nowhere.
fn aggregate_hooks(
    repo_claude_dir: &Path,
    repo_name: &str,
    repo_rel_path: &str,
    rows: &mut Vec<Row>,
    problems: &mut Vec<String>,
) -> Result<usize> {
    let src_dir = repo_claude_dir.join("hooks");
    if !src_dir.is_dir() {
        return Ok(0);
    }

    let per_repo_settings = read_settings_hooks(&repo_claude_dir.join("settings.json"));

    // Every file rather than every `.sh`, since a hook is whatever the host can
    // run, in any language. A hidden file is an editor's or a filesystem's, and
    // a directory is not a hook. Sorted, so a report reads the same twice.
    let mut names = Vec::new();
    for entry in fs::read_dir(&src_dir).with_context(|| format!("read {}", src_dir.display()))? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        match entry.file_name().to_str() {
            Some(n) if n.starts_with('.') => {},
            Some(n) => names.push(n.to_string()),
            None => {
                problems.push(format!(
                    "a file under `{repo_rel_path}/.claude/hooks/` has a name that is not \
                     UTF-8, so nothing could register it; not carried"
                ))
            },
        }
    }
    names.sort();

    let mut count = 0;
    'hooks: for name in names {
        let path = src_dir.join(&name);
        let shown = format!("{repo_rel_path}/.claude/hooks/{name}");
        let given = per_repo_settings.get(&name).filter(|r| !r.is_empty());
        // A registration that runs the hook through a program is how the host
        // runs a file with no execute bit, so only a hook nothing runs that way
        // has to carry one.
        let through_a_program = given.is_some_and(|r| r.iter().any(|x| !x.runner.is_empty()));
        if !through_a_program && !is_executable(&path) {
            problems.push(format!(
                "`{shown}` is not executable, so the host could not run it either; not carried"
            ));
            continue;
        }
        let registrations = match given {
            Some(r) => r.clone(),
            None => {
                match detect_matchers_from_hook_body(&path) {
                    Some(ms) => {
                        ms.into_iter()
                            .map(|matcher| {
                                Registration {
                                    event: "PreToolUse".into(),
                                    matcher,
                                    args: String::new(),
                                    runner: String::new(),
                                    timeout: None,
                                }
                            })
                            .collect()
                    },
                    None => {
                        problems.push(format!(
                            "`{shown}` is registered under no event in \
                             `{repo_rel_path}/.claude/settings.json` and names no `@matchers:`, \
                             so nothing would call it; not carried"
                        ));
                        continue;
                    },
                }
            },
        };

        let hook = format!("{ROOT}/{repo_rel_path}/.claude/hooks/{name}");
        let mut carried = Vec::new();
        for r in &registrations {
            let words = program_words(&r.runner, &shown).and_then(|mut run| {
                run.push(hook.clone());
                run.extend(argument_words(&r.args, &shown)?);
                Ok(run)
            });
            match words {
                Ok(run) => {
                    carried.push(Row {
                        event: r.event.clone(),
                        name: agent_shook::row_name(&format!("{repo_name}--{name}")),
                        run,
                        matcher: r.matcher.clone(),
                        paths: vec![repo_rel_path.to_string()],
                        at_root: false,
                        timeout: r.timeout.unwrap_or(agent_shook::DEFAULT_TIMEOUT),
                        filtered: true,
                    })
                },
                Err(problem) => {
                    problems.push(problem);
                    continue 'hooks;
                },
            }
        }
        rows.extend(carried);
        count += 1;
    }
    Ok(count)
}

/// One place a repository's `settings.json` calls a hook from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Registration {
    event:   String,
    /// Empty where the registration named none.
    matcher: String,
    /// Whatever followed the hook's path on its command line, carried onto the
    /// row so the hook still receives it.
    args:    String,
    /// Whatever preceded it, the program the host runs the hook with, which
    /// the row runs it with in turn. Empty where the host runs the file.
    runner:  String,
    /// Seconds the registration gave the hook, where it gave any.
    timeout: Option<u64>,
}

/// Map each hook file a repository's `settings.json` calls, by its name under
/// `.claude/hooks/`, to every event, matcher and argument list it is called
/// with: every event, and a registration naming no matcher as much as one
/// naming one. A command running nothing under `.claude/hooks/` is not about a
/// file this pass carries and is left out.
fn read_settings_hooks(path: &Path) -> BTreeMap<String, Vec<Registration>> {
    let mut out: BTreeMap<String, Vec<Registration>> = BTreeMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return out;
    };
    for (event, matcher, cmd, timeout) in registrations(&v) {
        let Some(call) = hook_call(cmd) else {
            continue;
        };
        let r = Registration {
            event: event.to_string(),
            matcher: matcher.to_string(),
            args: call.args,
            runner: call.runner,
            timeout,
        };
        let regs = out.entry(call.name).or_default();
        if !regs.contains(&r) {
            regs.push(r);
        }
    }
    out
}

/// Every hook a settings document registers, as its event, its matcher (empty where the entry
/// names none), its command line and the timeout it carries where it carries one, in the order
/// the document gives them. The one walk of a settings file's `hooks` table.
pub(crate) fn registrations(v: &serde_json::Value) -> Vec<(&str, &str, &str, Option<u64>)> {
    let mut out = Vec::new();
    let Some(events) = v.get("hooks").and_then(|h| h.as_object()) else {
        return out;
    };
    for (event, entries) in events {
        for entry in entries.as_array().into_iter().flatten() {
            let matcher = entry.get("matcher").and_then(|m| m.as_str()).unwrap_or("");
            let hooks = entry.get("hooks").and_then(|h| h.as_array());
            for h in hooks.into_iter().flatten() {
                if let Some(cmd) = h.get("command").and_then(|c| c.as_str()) {
                    out.push((
                        event.as_str(),
                        matcher,
                        cmd,
                        h.get("timeout").and_then(serde_json::Value::as_u64),
                    ));
                }
            }
        }
    }
    out
}

/// Fallback: parse the hook script's `# @matchers: ...` directive line.
/// Mockspace-emitted hooks carry this header for tooling consumption.
fn detect_matchers_from_hook_body(path: &Path) -> Option<Vec<String>> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines().take(20) {
        let l = line.trim_start_matches(|c: char| c == '#' || c.is_whitespace());
        if let Some(rest) = l.strip_prefix("@matchers:") {
            let names: Vec<String> = rest
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !names.is_empty() {
                return Some(names);
            }
        }
    }
    None
}

#[path = "aggregate_call.rs"]
mod call;
pub(crate) use call::{hook_call, hook_file_named, sh_single_quote_escape};

#[path = "aggregate_settings.rs"]
mod settings;
pub(crate) use settings::merge_settings;
#[cfg(test)]
use settings::{is_aggregated_command, is_retired_aggregated_command};

#[cfg(test)]
#[path = "aggregate_chains_tests.rs"]
pub(crate) mod chains_tests;

#[cfg(test)]
#[path = "aggregate_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "aggregate_registration_tests.rs"]
mod registration_tests;

#[cfg(test)]
#[path = "aggregate_words_tests.rs"]
mod words_tests;
