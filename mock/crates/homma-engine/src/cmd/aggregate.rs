//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Workspace-level aggregation of per-repo mockspace agent hooks.
//!
//! For each member repo with rendered hooks under `.claude/hooks/`, this
//! module:
//!
//! 1. Writes a wrapper for each at
//!    `<workspace>/.claude/hooks/<repo>--<name>.sh`, carrying
//!    [`MANAGED_MARK`]. Each wrapper reads the host's JSON from stdin, finds
//!    where the call lands, exits zero when that is not under the repo, and
//!    otherwise hands control to the real per-repo hook with the same stdin
//!    re-fed. Per-repo updates flow through automatically: the workspace
//!    wrapper is a thin scope check, and the substantive logic still lives in
//!    the repo.
//!
//! 2. Merges per-repo `settings.json` hook registrations into the workspace
//!    `.claude/settings.json`, under every event and matcher the repo gave
//!    each hook, with each command rewritten to the workspace wrapper.
//!    Registrations homma wrote before are swept first, so regens are
//!    idempotent. What is homma's is decided by the mark in the file a
//!    registration names, never by its name alone, so a hand-written hook
//!    and its registration are kept whatever they are called.
//!
//! A hook nothing registers, or one the host could not run, is reported by
//! path rather than written as a wrapper nobody calls.
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

/// One registration destined for the workspace `settings.json`: the event it
/// sits under, the tools it fires for, and the command the host runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HookEntry {
    pub event:   String,
    /// Empty writes no `matcher` at all, which the host reads as every tool,
    /// and which is how a registration that named none is carried.
    pub matcher: String,
    pub command: String,
}

impl HookEntry {
    /// The entry as the host reads it.
    fn to_json(&self) -> serde_json::Value {
        let hook = serde_json::json!({ "type": "command", "command": self.command });
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

/// The file a hook command runs, by its name under `.claude/hooks/`, whatever
/// prefix the command reaches it through: a relative path, the host's
/// project-root placeholder, or an absolute one. None for a command running
/// something elsewhere.
pub(crate) fn hook_file_named(cmd: &str) -> Option<String> {
    let first: String = cmd
        .split_whitespace()
        .next()?
        .chars()
        .filter(|c| *c != '"' && *c != '\'')
        .collect();
    let (_, name) = first.rsplit_once(".claude/hooks/")?;
    (!name.is_empty() && !name.contains('/')).then(|| name.to_string())
}

/// Where a call lands, as the jq expression every wrapper reads it with: the
/// file a file tool writes, then the directory the host says the session is
/// in, then a directory the call itself carries. Empty when none is there, and
/// the wrapper then falls back to its own working directory, which is the
/// workspace root whatever the session did.
pub(crate) const TARGET_JQ: &str =
    ".tool_input.file_path // .tool_input.path // .cwd // .tool_input.cwd // empty";

/// What one repository's pass carried and what it could not.
#[derive(Debug, Default)]
pub(crate) struct Aggregated {
    /// Wrappers written.
    pub hooks:    usize,
    /// Hooks not carried, each naming its path and why. Any of these fails the
    /// regen, since each is a hook no session opened at the root will run.
    pub problems: Vec<String>,
}

/// Aggregate one repo's hooks into the workspace `.claude/`, returning how
/// many wrappers were written and which hooks were not carried.
///
/// Rules are no longer aggregated; per-repo rules auto-load from the
/// repo's `.claude/rules/` when Claude Code is opened with cwd inside
/// the repo. Any previously-aggregated rules at
/// `<workspace>/.claude/rules/<repo>--*.md` get cleaned on every regen
/// so upgrades from older homma versions converge automatically.
///
/// `settings_entries` accumulates per-hook registrations that
/// [`merge_settings`] writes into the workspace `settings.json` after
/// the per-repo loop completes.
pub(crate) fn aggregate_repo(
    root: &Root,
    repo_name: &str,
    repo_abs_path: &Path,
    settings_entries: &mut Vec<HookEntry>,
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
    let ws_hooks = contain(root, ".claude/hooks")?;
    root.create_dir_all(&ws_rules).ok();
    root.create_dir_all(&ws_hooks).ok();

    // Cleans both prior-homma aggregated rules (kept after retirement
    // of rule aggregation) and prior-regen hook wrappers (the
    // idempotency guarantee for hooks).
    clean_stale(root, &ws_rules, repo_name, ".md", Sweep::Everything)?;
    clean_stale(root, &ws_hooks, repo_name, ".sh", Sweep::Marked)?;

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
        root,
        &claude_dir,
        &ws_hooks,
        repo_name,
        &repo_rel,
        settings_entries,
        &mut out.problems,
    )?;

    Ok(out)
}

/// Prove a workspace-relative path, naming what escaped when it does not.
fn contain(root: &Root, tail: &str) -> Result<ContainedPath> {
    root.contain(&root.as_abs().join(tail))
        .map_err(|e| anyhow!("{e}"))
}

/// Which files [`clean_stale`] may remove.
#[derive(Debug, Clone, Copy)]
enum Sweep {
    /// Every file carrying the prefix. Only the retired rule copies, which
    /// predate the mark and which nothing but homma ever named that way.
    Everything,
    /// Only the ones carrying [`MANAGED_MARK`]. Anything else under the same
    /// name is somebody's and stays.
    Marked,
}

/// Remove previously-aggregated files for `repo_name` so removed
/// per-repo entries do not linger at the workspace level.
fn clean_stale(
    root: &Root,
    dir: &ContainedPath,
    repo_name: &str,
    ext: &str,
    sweep: Sweep,
) -> Result<()> {
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
            if matches!(sweep, Sweep::Marked) && !carries_the_mark(target.as_path()) {
                continue;
            }
            root.remove_file(&target).ok();
        }
    }
    Ok(())
}

/// Walk the repository's `.claude/hooks/`, write a wrapper for every hook
/// something calls, and collect a registration for every event and matcher it
/// is called under. A file the host could not run, one nothing registers, and
/// one whose wrapper would land on a file homma did not write are each
/// reported in `problems` and carried nowhere.
fn aggregate_hooks(
    root: &Root,
    repo_claude_dir: &Path,
    dst_dir: &ContainedPath,
    repo_name: &str,
    repo_rel_path: &str,
    settings_entries: &mut Vec<HookEntry>,
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
        if !is_executable(&path) {
            problems.push(format!(
                "`{shown}` is not executable, so the host could not run it either; not carried"
            ));
            continue;
        }
        let registrations = match per_repo_settings.get(&name) {
            Some(r) if !r.is_empty() => r.clone(),
            _ => {
                match detect_matchers_from_hook_body(&path) {
                    Some(ms) => {
                        ms.into_iter()
                            .map(|matcher| {
                                Registration {
                                    event: "PreToolUse".into(),
                                    matcher,
                                    args: String::new(),
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

        // The wrapper is bash whatever the hook is written in, since all it
        // does is decide whether to hand off.
        let target_name = if name.ends_with(".sh") {
            format!("{repo_name}--{name}")
        } else {
            format!("{repo_name}--{name}.sh")
        };
        let target_path = root
            .contain_under(dst_dir, &target_name)
            .map_err(|e| anyhow!("{e}"))?;
        if target_path.as_path().exists() && !carries_the_mark(target_path.as_path()) {
            problems.push(format!(
                "`.claude/hooks/{target_name}` was not written by homma, so `{shown}` is not \
                 carried over it; move that file aside or rename it"
            ));
            continue;
        }

        let stem_path = format!(".claude/hooks/{name}");
        let wrapper = wrapper_script(repo_name, repo_rel_path, &stem_path);
        root.write(&target_path, wrapper)
            .with_context(|| format!("write {}", target_path.as_path().display()))?;
        #[cfg(unix)]
        root.set_executable(&target_path)?;

        // `${CLAUDE_PROJECT_DIR}` rather than the path this run happened to
        // write to. The host substitutes it for the project root "regardless of
        // the working directory when the hook runs", which is what makes a
        // tracked `settings.json` name this workspace's wrappers in every
        // clone. The absolute form it replaces named the workspace that
        // generated the file, so every other clone either could not find the
        // command at all or, on the same machine, ran somebody else's copy.
        let command = format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{target_name}");
        for r in registrations {
            let command = if r.args.is_empty() {
                command.clone()
            } else {
                format!("{command} {}", r.args)
            };
            settings_entries.push(HookEntry {
                event: r.event,
                matcher: r.matcher,
                command,
            });
        }

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
    /// wrapper's command so the hook still receives it.
    args:    String,
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
                let Some(name) = hook_file_named(cmd) else {
                    continue;
                };
                let args = cmd
                    .trim()
                    .split_once(char::is_whitespace)
                    .map(|(_, a)| a.trim().to_string())
                    .unwrap_or_default();
                let r = Registration {
                    event: event.clone(),
                    matcher: matcher.clone(),
                    args,
                };
                let regs = out.entry(name).or_default();
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

/// Escape a string for embedding inside a bash single-quoted literal.
/// Replaces each `'` with `'\''` so the generated script is safe for
/// paths or names that happen to contain a single quote.
pub(crate) fn sh_single_quote_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Build the wrapper script body for an aggregated hook.
///
/// Both paths are relative, and that is the whole of this function.
/// `repo_rel_path` is the repo's path under the workspace, which the manifest
/// already holds for the detected member; `hook_rel_path` is the hook's
/// path under the repo. Neither names a machine.
///
/// A wrapper sits at `<workspace>/.claude/hooks/<file>`, a fixed depth, so it
/// finds the workspace from its own location and needs no baked prefix and no
/// environment variable. That is what lets a tracked wrapper work in every
/// clone rather than only in the one that generated it. The shape it replaces
/// baked the generating workspace's absolute path, which made every wrapper
/// inert in every other clone: the scope check matched nothing and the wrapper
/// exited 0, indistinguishable from a guard that ran and approved.
///
/// The emitted wrapper:
/// 1. Locates the workspace from its own path and derives the repo root and the
///    real hook under it.
/// 2. Exits 0 when that hook is not executable, which is the case where this
///    workspace has not cloned the repo. There is no guard to run, so it
///    declines rather than reporting an approval it did not make.
/// 3. Reads the host's JSON on stdin and finds where the call lands, by
///    [`TARGET_JQ`], falling back to `$PWD` when nothing in it says.
/// 4. Exits 0 when the target is not under the repo root.
/// 5. Otherwise replaces itself with the real hook, passing on its own
///    arguments and re-feeding the original stdin.
pub(crate) fn wrapper_script(repo_name: &str, repo_rel_path: &str, hook_rel_path: &str) -> String {
    let repo_rel = sh_single_quote_escape(repo_rel_path);
    let hook_rel = sh_single_quote_escape(hook_rel_path);
    let target_jq = TARGET_JQ;
    format!(
        r##"#!/usr/bin/env bash
# Aggregated from `{repo_name}` by `homma agent regen`.
# Scoped to {repo_rel_path}, relative to the workspace this file sits in.
# Source hook: {repo_rel_path}/{hook_rel_path}

set -u

WS=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")/../.." && pwd) || exit 0
REPO_REL='{repo_rel}'
case "$REPO_REL" in
    /*) REPO_ROOT="$REPO_REL" ;;
    *)  REPO_ROOT="$WS/$REPO_REL" ;;
esac
ORIG_HOOK="$REPO_ROOT"'/{hook_rel}'

# Not cloned in this workspace: there is no guard here to run, and declining is
# the honest answer rather than an approval nobody made.
[ -x "$ORIG_HOOK" ] || exit 0

INPUT=$(cat)

# Without `jq` this cannot read which path is being written, so it cannot narrow
# to this repo. It forwards instead of guessing: forwarding is what happens with
# no aggregation at all, so the guard runs more often than it needs to and never
# silently fails to run. Guessing `$PWD` is what it used to do, and that skips
# the guard for every write outside the directory the caller happens to be in.
if ! command -v jq >/dev/null 2>&1; then
    exec "$ORIG_HOOK" "$@" <<<"$INPUT"
fi

target=$(printf '%s' "$INPUT" | jq -r '{target_jq}' 2>/dev/null)

if [ -z "$target" ]; then
    target="$PWD"
fi

case "$target" in
    "$REPO_ROOT"|"$REPO_ROOT"/*) ;;
    *) exit 0 ;;
esac

exec "$ORIG_HOOK" "$@" <<<"$INPUT"
"##
    )
}

#[path = "aggregate_settings.rs"]
mod settings;
pub(crate) use settings::merge_settings;
#[cfg(test)]
use settings::{is_aggregated_command, is_retired_aggregated_command};

#[cfg(test)]
#[path = "aggregate_chains_tests.rs"]
pub(crate) mod chains_tests;

#[cfg(test)]
pub(crate) mod tests {

    /// A `Root` over a test workspace, denying nothing that a test uses.
    ///
    /// The real code path, not a variant of it: these go through
    /// `Root::contain` exactly as production does, which is what makes the
    /// containment they assert mean anything.
    pub(crate) fn test_root(workspace: &Path) -> Root {
        Root::new(
            &homma_api::AbsPath::new(workspace).expect("a tempdir path is absolute"),
            homma_api::Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap()),
        )
        .expect("a tempdir is a legitimate root")
    }

    /// Mark a file executable, which the wrapper's own `[ -x ]` check reads.
    pub(crate) fn make_executable(p: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = fs::metadata(p).unwrap().permissions();
            perm.set_mode(0o755);
            fs::set_permissions(p, perm).unwrap();
        }
    }

    /// Run an emitted wrapper with a tool-input payload naming `target`.
    fn run_wrapper(wrapper: &Path, target: &Path) {
        run_wrapper_output(wrapper, target);
    }

    /// As `run_wrapper`, keeping what the process said and how it exited.
    fn run_wrapper_output(wrapper: &Path, target: &Path) -> std::process::Output {
        run_wrapper_on_path(wrapper, target, None)
    }

    /// The same, with `path` replacing `PATH` for the child.
    ///
    /// A machine without `jq` is reproduced on one that has it by handing over
    /// a directory holding only `bash`.
    fn run_wrapper_on_path(
        wrapper: &Path,
        target: &Path,
        path: Option<&str>,
    ) -> std::process::Output {
        use std::io::Write;
        let payload = format!(r#"{{"tool_input":{{"file_path":"{}"}}}}"#, target.display());
        let mut cmd = std::process::Command::new("bash");
        cmd.arg(wrapper)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(p) = path {
            cmd.env("PATH", p);
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    use super::*;

    #[test]
    fn a_wrapper_carries_no_absolute_path_and_finds_its_workspace_from_its_own_location() {
        let s = wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh");
        assert!(s.starts_with("#!/usr/bin/env bash"));
        assert!(s.contains("REPO_REL='arvo'"));
        assert!(s.contains("ORIG_HOOK=\"$REPO_ROOT\"'/.claude/hooks/foo.sh'"));
        assert!(s.contains("BASH_SOURCE[0]"));
        assert!(s.contains("$ORIG_HOOK"));
        assert!(s.contains("Aggregated from `arvo`"));

        // The whole point, and the assertion the old shape could not have
        // passed: nothing in the emitted script names a machine. A path
        // starting at the filesystem root is a fact about the workspace that
        // generated the file and is inert in every other clone.
        for line in s.lines() {
            let code = line.split('#').next().unwrap_or(line);
            assert!(
                !code.contains("='/") && !code.contains("=\"/"),
                "wrapper assigns an absolute path: {line}"
            );
        }
    }

    #[test]
    fn a_wrapper_matches_a_path_under_its_repo_and_declines_one_outside() {
        // The emitted script, run. A unit test over its text can only say the
        // strings are there; whether the scope check fires is a property of
        // bash, and bash is available.
        let ws = tempfile::tempdir().unwrap();
        let hooks = ws.path().join(".claude/hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::create_dir_all(ws.path().join("arvo/.claude/hooks")).unwrap();

        // The real hook records that it ran, so "did the wrapper hand off" is
        // observable rather than inferred from an exit code that is 0 either
        // way.
        let marker = ws.path().join("fired");
        let real = ws.path().join("arvo/.claude/hooks/foo.sh");
        fs::write(
            &real,
            format!(
                "#!/usr/bin/env bash\ncat > /dev/null\ntouch '{}'\n",
                marker.display()
            ),
        )
        .unwrap();
        make_executable(&real);

        let wrapper = hooks.join("arvo--foo.sh");
        fs::write(
            &wrapper,
            wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh"),
        )
        .unwrap();
        make_executable(&wrapper);

        let inside = ws.path().join("arvo/src/lib.rs");
        run_wrapper(&wrapper, &inside);
        assert!(
            marker.exists(),
            "the wrapper did not hand off for a path inside the repo"
        );

        // The control. Without it, a wrapper that handed off unconditionally
        // would pass the assertion above and be exactly the guard-shaped thing
        // that guards nothing.
        fs::remove_file(&marker).unwrap();
        let outside = ws.path().join("kolli/src/lib.rs");
        run_wrapper(&wrapper, &outside);
        assert!(
            !marker.exists(),
            "the wrapper handed off for a path outside the repo"
        );
    }

    /// **Without `jq` the wrapper hands off rather than guessing.**
    ///
    /// It read the target path with `jq` and fell back to `$PWD` when that came
    /// back empty, so on a machine without `jq` the scope check was against the
    /// caller's directory rather than against the write. A write inside the repo
    /// from anywhere else silently skipped the repo's own hook, which is a guard
    /// that does not run reporting nothing.
    ///
    /// Handing off is the safe direction: it is what happens with no
    /// aggregation at all, so the cost of being wrong is a hook running when it
    /// need not rather than one that never runs.
    #[test]
    fn a_wrapper_without_jq_hands_off_rather_than_guessing_from_the_directory() {
        let ws = tempfile::tempdir().unwrap();
        let hooks = ws.path().join(".claude/hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::create_dir_all(ws.path().join("arvo/.claude/hooks")).unwrap();

        // Builtins only, because this runs under a PATH holding one program.
        // `printf` and the redirection are bash's own; `cat` and `touch` are
        // not, and the first version of this fixture used both and failed for
        // that rather than for the property under test.
        let marker = ws.path().join("fired");
        let real = ws.path().join("arvo/.claude/hooks/foo.sh");
        fs::write(
            &real,
            format!("#!/usr/bin/env bash\nprintf '' > '{}'\n", marker.display()),
        )
        .unwrap();
        make_executable(&real);

        let wrapper = hooks.join("arvo--foo.sh");
        fs::write(
            &wrapper,
            wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh"),
        )
        .unwrap();
        make_executable(&wrapper);

        let bare = crate::cmd::gates::tests::a_path_without_jq(ws.path());

        // The write is inside the repo, and the working directory is not, which
        // is the exact shape the `$PWD` fallback got wrong.
        let inside = ws.path().join("arvo/src/lib.rs");
        run_wrapper_on_path(&wrapper, &inside, Some(&bare));
        assert!(
            marker.exists(),
            "a write inside the repo skipped the repo's own hook when jq was absent"
        );

        // The control: with jq present the same call also hands off, so the
        // assertion above is about the missing tool and not about the wrapper
        // handing off unconditionally in every configuration.
        fs::remove_file(&marker).unwrap();
        let outside = ws.path().join("kolli/src/lib.rs");
        run_wrapper(&wrapper, &outside);
        assert!(
            !marker.exists(),
            "control: with jq present a path outside the repo is still declined"
        );
    }

    #[test]
    fn a_wrapper_declines_when_the_repo_is_not_cloned_here() {
        // A tracked wrapper travels to workspaces holding a different subset of
        // the manifest. There is no guard to run there.
        //
        // The target has to be INSIDE the absent repo, and that is the whole of
        // this test. A path outside it exits 0 through the scope check whether
        // or not the executable check exists, so a test aiming there passes
        // against the defect and measures nothing. Inside, the two arms
        // separate: with the check the wrapper declines, and without it the
        // wrapper reaches `exec` on a file that is not there and fails.
        let ws = tempfile::tempdir().unwrap();
        let hooks = ws.path().join(".claude/hooks");
        fs::create_dir_all(&hooks).unwrap();
        // deliberately: no `arvo/` in this workspace

        let wrapper = hooks.join("arvo--foo.sh");
        fs::write(
            &wrapper,
            wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh"),
        )
        .unwrap();
        make_executable(&wrapper);

        let inside = ws.path().join("arvo/src/lib.rs");
        let out = run_wrapper_output(&wrapper, &inside);
        assert!(
            out.status.success(),
            "a wrapper with nothing to run failed instead of declining: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "a wrapper with nothing to run said something: out={} err={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_relative_repo_root_never_matches_the_absolute_path_the_host_supplies() {
        // Why the shape this replaces was inert even in the workspace that
        // generated it, when the manifest was reached by a relative path. The
        // host always supplies an absolute `file_path`, and the wrapper's
        // comparison is textual.
        let matches = |root: &str, target: &str| -> bool {
            let out = std::process::Command::new("bash")
                .arg("-c")
                .arg(format!(
                    r#"case "{target}" in "{root}"|"{root}"/*) exit 0;; *) exit 1;; esac"#
                ))
                .output()
                .unwrap();
            out.status.success()
        };
        assert!(!matches("./arvo", "/ws/arvo/src/lib.rs"));
        // Two controls, so the assertion above is about the relative form
        // rather than about the comparison never matching anything.
        assert!(matches("./arvo", "./arvo/src/lib.rs"));
        assert!(matches("/ws/arvo", "/ws/arvo/src/lib.rs"));
    }

    #[test]
    fn aggregated_command_detected_by_prefix() {
        let cmd = ".claude/hooks/arvo--no-alloc-guard.sh";
        assert!(is_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
    }

    #[test]
    fn non_aggregated_command_not_detected() {
        let cmd = ".claude/hooks/workspace-only.sh";
        assert!(!is_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn legacy_imports_path_detected_as_aggregated() {
        // Pre-homma bash aggregator wrote per-repo hooks under
        // `imports/<repo>/<name>.sh` rather than the current flat
        // `<repo>--<name>.sh` convention. Entries left over from that
        // era must still get swept out on regen.
        let cmd = ".claude/hooks/imports/arvo/no-alloc-guard.sh";
        assert!(is_retired_aggregated_command(cmd, &[
            "arvo",
            "hilavitkutin"
        ]));
        // and the current-shape check does not claim it, so the two are not
        // silently the same predicate under two names
        assert!(!is_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
    }

    #[test]
    fn legacy_imports_at_path_start_detected_as_aggregated() {
        // Relative path starting with `imports/<repo>/` (no leading
        // separator). Must still match the legacy pattern.
        let cmd = "imports/arvo/no-alloc-guard.sh";
        assert!(is_retired_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn an_absolute_managed_command_is_retired_whatever_repo_it_names() {
        let cmd = "/Users/someone/Dev/their-workspace/.claude/hooks/arvo--no-alloc-guard.sh";
        assert!(is_retired_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn a_placeholder_command_for_an_unvisited_repo_is_not_retired() {
        // The control that keeps the preservation rule alive. A predicate that
        // swept both shapes would satisfy the test above and quietly undo it.
        let cmd = "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--no-alloc-guard.sh";
        assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn an_unmanaged_absolute_command_is_left_alone() {
        // The second control, and the worst thing this could get wrong: a
        // hand-authored user-level hook is absolute too.
        let cmd = "/Users/someone/.claude/hooks/self-compact-trigger.sh";
        assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
        assert!(!is_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn imports_substring_not_at_path_boundary_not_detected() {
        // A command that happens to contain the substring `imports/arvo/`
        // in the middle of a longer path component must NOT be flagged.
        // Path-component anchoring prevents false positives on
        // e.g. user-authored paths like `myimports/arvo/foo.sh`.
        let cmd = ".claude/hooks/myimports/arvo/foo.sh";
        assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn matcher_detection_from_hook_body_directive() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hook.sh");
        fs::write(
            &p,
            "#!/usr/bin/env bash\n# @matchers: Write, Edit\n# Some hook.\n",
        )
        .unwrap();
        let m = detect_matchers_from_hook_body(&p).unwrap();
        assert_eq!(m, vec!["Write".to_string(), "Edit".to_string()]);
    }

    #[test]
    fn aggregate_repo_end_to_end_against_synthetic_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        let repo_abs = workspace.join("arvo");
        fs::create_dir_all(repo_abs.join(".claude/rules")).unwrap();
        fs::create_dir_all(repo_abs.join(".claude/hooks")).unwrap();

        // Stale aggregated rule from a prior homma version: must get
        // swept out on regen even though no new rule is being written.
        fs::create_dir_all(workspace.join(".claude/rules")).unwrap();
        fs::write(
            workspace.join(".claude/rules/arvo--stale-rule.md"),
            "---\npaths:\n  - \"arvo/**\"\n---\nLeftover.\n",
        )
        .unwrap();

        // Per-repo source rules still live in the repo; homma no longer
        // copies them. Verify by writing one and checking it does NOT
        // appear in the workspace .claude/rules/ post-aggregate.
        fs::write(
            repo_abs.join(".claude/rules/type-surface.md"),
            "---\npaths:\n  - \"crates/**/*.rs\"\n---\nBody.\n",
        )
        .unwrap();
        fs::write(
            repo_abs.join(".claude/hooks/no-alloc.sh"),
            "#!/usr/bin/env bash\n# @matchers: Write, Edit\necho hi\n",
        )
        .unwrap();
        make_executable(&repo_abs.join(".claude/hooks/no-alloc.sh"));
        fs::write(
            repo_abs.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/no-alloc.sh"}]}]}}"#,
        )
        .unwrap();

        let mut settings = Vec::new();
        let a = aggregate_repo(&test_root(workspace), "arvo", &repo_abs, &mut settings).unwrap();
        assert_eq!(a.hooks, 1);
        assert!(a.problems.is_empty(), "{:?}", a.problems);

        // Stale aggregated rule was cleaned.
        assert!(
            !workspace.join(".claude/rules/arvo--stale-rule.md").exists(),
            "stale aggregated rule should have been cleaned by clean_stale"
        );
        // Repo-side rule was NOT propagated.
        assert!(
            !workspace
                .join(".claude/rules/arvo--type-surface.md")
                .exists(),
            "homma no longer aggregates per-repo rules"
        );

        let hook = fs::read_to_string(workspace.join(".claude/hooks/arvo--no-alloc.sh")).unwrap();
        assert!(hook.contains("REPO_REL='arvo'"));
        assert!(hook.contains("ORIG_HOOK="));
        assert!(
            !hook.contains(workspace.to_str().unwrap()),
            "the wrapper baked the generating workspace's path"
        );

        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].event, "PreToolUse");
        assert_eq!(settings[0].matcher, "Edit");
        assert_eq!(
            settings[0].command, "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--no-alloc.sh",
            "the registered command must name the host's project-root placeholder rather \
             than this run's workspace",
        );
        assert!(
            !settings[0].command.contains(workspace.to_str().unwrap()),
            "expected no generating-workspace path in the command, got: {}",
            settings[0].command,
        );

        merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &settings, None).unwrap();
        let written = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "Edit");
        let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.ends_with("/.claude/hooks/arvo--no-alloc.sh"),
            "expected absolute path, got: {cmd}",
        );
    }

    #[test]
    fn merge_settings_preserves_hand_authored_entries() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}]}}"#,
        )
        .unwrap();

        let entries = vec![HookEntry {
            event:   "PreToolUse".into(),
            matcher: "Edit".into(),
            command: ".claude/hooks/arvo--no-alloc.sh".into(),
        }];
        merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(
            arr.iter()
                .any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh")
        );
        assert!(
            arr.iter()
                .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--no-alloc.sh")
        );
    }

    #[test]
    fn merge_settings_replaces_previously_aggregated() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[
                {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/arvo--old.sh"}]},
                {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}
            ]}}"#,
        )
        .unwrap();

        let entries = vec![HookEntry {
            event:   "PreToolUse".into(),
            matcher: "Write".into(),
            command: ".claude/hooks/arvo--new.sh".into(),
        }];
        merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(
            !arr.iter()
                .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--old.sh")
        );
        assert!(
            arr.iter()
                .any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh")
        );
        assert!(
            arr.iter()
                .any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--new.sh")
        );
    }

    #[test]
    fn merge_settings_preserves_mixed_hook_entries() {
        // An entry with one aggregated hook AND one hand-authored hook
        // in the same `hooks[]` array. Per-hook filtering must strip
        // only the aggregated hook and keep the entry intact with the
        // hand-authored hook surviving.
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[
                {"matcher":"Edit","hooks":[
                    {"type":"command","command":".claude/hooks/arvo--old.sh"},
                    {"type":"command","command":".claude/hooks/workspace-handauthored.sh"}
                ]}
            ]}}"#,
        )
        .unwrap();

        let entries = vec![HookEntry {
            event:   "PreToolUse".into(),
            matcher: "Write".into(),
            command: ".claude/hooks/arvo--new.sh".into(),
        }];
        merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &entries, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        // Original Edit entry preserved, with `arvo--old.sh` stripped
        // and `workspace-handauthored.sh` surviving. Plus the freshly
        // pushed `arvo--new.sh`.
        assert_eq!(arr.len(), 2);
        let edit = arr.iter().find(|e| e["matcher"] == "Edit").unwrap();
        let edit_hooks = edit["hooks"].as_array().unwrap();
        assert_eq!(
            edit_hooks.len(),
            1,
            "aggregated hook should be stripped, hand-authored preserved"
        );
        assert_eq!(
            edit_hooks[0]["command"],
            ".claude/hooks/workspace-handauthored.sh"
        );
        let write = arr.iter().find(|e| e["matcher"] == "Write").unwrap();
        assert_eq!(write["hooks"][0]["command"], ".claude/hooks/arvo--new.sh");
    }

    #[test]
    fn merge_settings_keeps_entries_for_a_known_repo_this_run_did_not_visit() {
        // A workspace clones the repos its work touches, so most of the
        // manifest aggregates nothing on any given run. Sweeping on the full
        // manifest deleted those registrations, and the wrapper files survived
        // because the cleanup runs inside the per-repo pass that was skipped.
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[
                {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/kolli--guard.sh"}]},
                {"matcher":"Edit","hooks":[{"type":"command","command":"/elsewhere/.claude/hooks/kolli--stale.sh"}]},
                {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--old.sh"}]}
            ]}}"#,
        )
        .unwrap();

        let entries = vec![HookEntry {
            event:   "PreToolUse".into(),
            matcher: "Edit".into(),
            command: "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--new.sh".into(),
        }];
        merge_settings(
            &test_root(workspace),
            &["arvo", "kolli"],
            &["arvo"],
            &entries,
            None,
        )
        .unwrap();

        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let cmds: Vec<String> = v["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|e| e["hooks"].as_array().unwrap())
            .map(|h| h["command"].as_str().unwrap().to_string())
            .collect();

        assert!(
            cmds.iter().any(|c| c.ends_with("kolli--guard.sh")),
            "an unvisited repo's registration was swept: {cmds:?}"
        );
        // The control on the same run: the repo that WAS visited is rewritten,
        // so preservation is not the whole predicate.
        assert!(
            !cmds.iter().any(|c| c.ends_with("arvo--old.sh")),
            "a visited repo's stale registration survived: {cmds:?}"
        );
        assert!(cmds.iter().any(|c| c.ends_with("arvo--new.sh")));
        // And the unvisited repo's *absolute* entry goes, because no clone can
        // resolve it. Preservation is about the placeholder form only.
        assert!(
            !cmds.iter().any(|c| c.ends_with("kolli--stale.sh")),
            "a retired absolute entry survived on an unvisited repo: {cmds:?}"
        );
    }

    #[test]
    fn merge_settings_sweeps_the_legacy_shape_for_any_known_repo_visited_or_not() {
        // The retired bash aggregator's `imports/<repo>/` form. Nothing writes
        // it any more, so there is no workspace where keeping it makes it work,
        // and it is swept on the full manifest rather than on the visited set.
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[
                {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/imports/kolli/guard.sh"}]},
                {"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
            ]}}"#,
        )
        .unwrap();

        merge_settings(
            &test_root(workspace),
            &["arvo", "kolli"],
            &["arvo"],
            &[],
            None,
        )
        .unwrap();

        let body = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
        assert!(
            !body.contains("imports/kolli"),
            "legacy entry survived: {body}"
        );
        // The control: a hand-authored entry is not swept alongside it.
        assert!(
            body.contains("mine.sh"),
            "hand-authored entry was swept: {body}"
        );
    }

    #[test]
    fn merge_settings_drops_entry_when_all_hooks_aggregated() {
        // Inverse of the mixed-hook test: when every hook in an entry
        // is aggregated, the entry collapses to empty and gets dropped.
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();
        fs::create_dir_all(workspace.join(".claude")).unwrap();
        fs::write(
            workspace.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[
                {"matcher":"Edit","hooks":[
                    {"type":"command","command":".claude/hooks/arvo--old1.sh"},
                    {"type":"command","command":".claude/hooks/arvo--old2.sh"}
                ]},
                {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/workspace-byline.sh"}]}
            ]}}"#,
        )
        .unwrap();

        merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &[], None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        // Edit entry collapsed (all hooks were aggregated); Bash entry
        // survives.
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "Bash");
    }
}
