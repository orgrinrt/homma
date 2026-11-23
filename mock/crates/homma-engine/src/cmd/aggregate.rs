//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Workspace-level aggregation of per-repo mockspace agent hooks.
//!
//! For each member repo with rendered hooks under `.claude/hooks/`, this
//! module makes an [`Entry`] for the dispatcher under every event and matcher
//! the repo's `settings.json` gives each hook, with the timeout and `async` it
//! gives it, narrowed to the repo. The dispatcher runs the real per-repo hook,
//! so per-repo updates flow through without a regen. `dispatch.rs` writes the
//! table and the dispatcher, and [`merge_settings`] registers it in the
//! clone's `.claude/settings.local.json`, sweeping what homma registered
//! before. What is homma's is decided by the mark in the file a registration
//! names, never by its name alone, so a hand-written hook and its registration
//! are kept whatever they are called.
//!
//! A hook nothing registers, or one the host could not run, is reported by
//! path rather than made an entry nothing matches.
//!
//! Rules are deliberately NOT aggregated. Per-repo rules auto-load when
//! Claude Code is opened with cwd inside the repo; rule aggregation at
//! the workspace level produced confused doubling (the same rule loading
//! once from each source). Skills are also not aggregated. Claude Code
//! skills do not support `paths:`-scoped activation, so workspace-level
//! skills would load unconditionally and the scoping property would
//! not hold.
//!
//! Legacy aggregated rules at `<workspace>/.claude/rules/<repo>--*.md`
//! get cleaned on every regen via [`clean_stale`] so upgrades from
//! older homma versions converge to the current shape automatically.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use homma_api::{ContainedPath, Root};
use serde::Serialize;

use crate::cmd::dispatch::Entry;

/// One registration destined for the clone's `settings.local.json`: the event it
/// sits under, the tools it fires for, the command the host runs, the seconds
/// the host gives it and whether the call waits on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HookEntry {
    pub event:      String,
    /// Empty writes no `matcher` at all, which the host reads as every tool.
    pub matcher:    String,
    pub command:    String,
    /// None writes no `timeout`, which leaves the host's default.
    pub timeout:    Option<u32>,
    /// Written as `async`, which the host runs in the background.
    pub background: bool,
}

impl HookEntry {
    /// The entry as the host reads it.
    fn to_json(&self) -> serde_json::Value {
        let mut hook = serde_json::json!({ "type": "command", "command": self.command });
        if let Some(t) = self.timeout {
            hook["timeout"] = serde_json::json!(t);
        }
        if self.background {
            hook["async"] = serde_json::json!(true);
        }
        if self.matcher.is_empty() {
            serde_json::json!({ "hooks": [hook] })
        } else {
            serde_json::json!({ "matcher": self.matcher, "hooks": [hook] })
        }
    }
}

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

/// Make the dispatcher's entries for one repo's hooks, returning how many hooks
/// were carried and which were not.
///
/// Rules are no longer aggregated; per-repo rules auto-load from the
/// repo's `.claude/rules/` when Claude Code is opened with cwd inside
/// the repo. Any previously-aggregated rules at
/// `<workspace>/.claude/rules/<repo>--*.md` get cleaned on every regen
/// so upgrades from older homma versions converge automatically.
///
/// `entries` accumulates the entries; a hook whose registration names no
/// timeout gets `timeout`.
pub(crate) fn aggregate_repo(
    root: &Root,
    repo_name: &str,
    repo_abs_path: &Path,
    timeout: u32,
    entries: &mut Vec<Entry>,
) -> Result<Aggregated> {
    let claude_dir = repo_abs_path.join(".claude");
    if !claude_dir.is_dir() {
        return Err(anyhow!(
            "repo `{repo_name}` has no .claude/ at {}; run `cargo mock` in the repo first",
            claude_dir.display(),
        ));
    }

    // **A `Root` rather than a `&Path`, and that is the whole of the eighth
    // relocation.** A previous round checked `<workspace>/.claude` as one string
    // and left every path below it built with `Path::join`, which resolves
    // nothing: a symlink one component down carried these writes into the
    // operator's own `.claude`, deleted files there and installed executables,
    // at exit 0 printing `regen: ok`.
    //
    // What has to be proven is the path `std::fs` receives, not a directory
    // above it. `org up` has gone through this mechanism for several rounds;
    // this pass had a hand-rolled prefix check instead.
    let ws_rules = contain(root, ".claude/rules")?;
    root.create_dir_all(&ws_rules).ok();

    // The rules homma aggregated before it stopped; the wrappers it wrote
    // before the dispatcher are removed when the dispatcher is written.
    clean_stale(root, &ws_rules, repo_name, ".md")?;

    // The workspace-relative path is the portable half and the only half the
    // wrappers may carry. A repo declared outside the workspace has none, and
    // there the absolute path is the only shape available; it stays correct for
    // this workspace and travels no worse than the old shape did.
    let repo_rel = repo_abs_path
        .strip_prefix(root.as_abs())
        .unwrap_or(repo_abs_path);
    // Through `relative_str`, so a manifest writing `./arvo` does not put a
    // `.` in the middle of the path the wrapper compares against.
    let repo_rel = crate::cmd::util::relative_str(repo_rel);

    let mut out = Aggregated::default();
    out.hooks = aggregate_hooks(
        &claude_dir,
        repo_name,
        &repo_rel,
        timeout,
        entries,
        &mut out.problems,
    )?;

    Ok(out)
}

/// Prove a workspace-relative path, naming what escaped when it does not.
fn contain(root: &Root, tail: &str) -> Result<ContainedPath> {
    root.contain(&root.as_abs().join(tail))
        .map_err(|e| anyhow!("{e}"))
}

/// Remove the rule copies homma aggregated for `repo_name` before it stopped
/// aggregating rules. Every file carrying the prefix goes: they predate the
/// mark, and nothing but homma ever named a rule that way.
fn clean_stale(root: &Root, dir: &ContainedPath, repo_name: &str, ext: &str) -> Result<()> {
    if !dir.as_path().is_dir() {
        return Ok(());
    }
    let prefix = format!("{repo_name}--");
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

/// Walk the repository's `.claude/hooks/` and make an entry for every event and
/// matcher a hook something calls is called under. A file the host could not
/// run, one nothing registers, and one two registrations run two different ways
/// are each reported in `problems` and carried nowhere.
fn aggregate_hooks(
    repo_claude_dir: &Path,
    repo_name: &str,
    repo_rel_path: &str,
    timeout: u32,
    entries: &mut Vec<Entry>,
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
    for name in names {
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
                                    background: false,
                                    unsupported: Vec::new(),
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

        let runner = match one_runner(registrations.iter().map(|r| r.runner.as_str()), &shown) {
            Ok(r) => r,
            Err(problem) => {
                problems.push(problem);
                continue;
            },
        };
        if let Some(r) = registrations.iter().find(|r| !r.unsupported.is_empty()) {
            problems.push(format!(
                "`{shown}` is registered with `{}`, which the dispatcher does not carry, so it \
                 would run otherwise than its repository wrote; not carried",
                r.unsupported.join("`, `")
            ));
            continue;
        }
        if let Some(problem) = registrations
            .iter()
            .find_map(|r| plain_args(&r.args, &shown).err())
        {
            problems.push(problem);
            continue;
        }

        // Workspace-relative, the portable half, so the table names no machine
        // and stays right when the clone is moved or copied.
        let run = format!("{repo_rel_path}/.claude/hooks/{name}");
        for r in registrations {
            entries.push(Entry {
                event:      r.event,
                matcher:    r.matcher,
                run:        run.clone(),
                runner:     runner.clone(),
                args:       r.args,
                roots:      vec![repo_rel_path.to_string()],
                timeout:    r.timeout.unwrap_or(timeout),
                background: r.background,
                from:       repo_name.to_string(),
            });
        }

        count += 1;
    }
    Ok(count)
}

/// One place a repository's `settings.json` calls a hook from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Registration {
    event:       String,
    /// Empty where the registration named none.
    matcher:     String,
    /// Whatever followed the hook's path on its command line, carried onto the
    /// wrapper's command so the hook still receives it.
    args:        String,
    /// Whatever preceded it, the program the host runs the hook with, which
    /// the dispatcher runs it with in turn. Empty where the host runs the file.
    runner:      String,
    /// The seconds the registration gives it, where it gives any.
    timeout:     Option<u32>,
    /// Whether the registration runs it `async`.
    background:  bool,
    /// The handler fields it gives that the dispatcher does not carry, by name.
    unsupported: Vec<String>,
}

/// The fields of a command handler the dispatcher carries; any other is named
/// in [`Registration::unsupported`].
const CARRIED: &[&str] = &["type", "command", "timeout", "async"];

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
    let Some(events) = v.get("hooks").and_then(|h| h.as_object()) else {
        return out;
    };
    for (event, entries) in events {
        let Some(arr) = entries.as_array() else {
            continue;
        };
        for entry in arr {
            let matcher = entry
                .get("matcher")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let Some(hooks) = entry.get("hooks").and_then(|h| h.as_array()) else {
                continue;
            };
            for h in hooks {
                let Some(cmd) = h.get("command").and_then(|c| c.as_str()) else {
                    continue;
                };
                let Some(call) = hook_call(cmd) else {
                    continue;
                };
                let r = Registration {
                    event:       event.clone(),
                    matcher:     matcher.clone(),
                    args:        call.args,
                    runner:      call.runner,
                    timeout:     h
                        .get("timeout")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|t| u32::try_from(t).ok())
                        .filter(|t| *t > 0),
                    background:  h.get("async").and_then(serde_json::Value::as_bool) == Some(true),
                    unsupported: h
                        .as_object()
                        .map(|o| {
                            o.keys()
                                .filter(|k| !CARRIED.contains(&k.as_str()))
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                let regs = out.entry(call.name).or_default();
                if !regs.contains(&r) {
                    regs.push(r);
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

#[path = "aggregate_wrapper.rs"]
pub(in crate::cmd) mod wrapper;
pub(crate) use wrapper::{
    LANDS_IN_SH,
    hook_call,
    hook_file_named,
    one_runner,
    plain_args,
    sh_single_quote_escape,
};

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
