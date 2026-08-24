//! Workspace-level aggregation of per-repo mockspace agent hooks.
//!
//! For each member repo with rendered `.claude/hooks/*.sh`, this module:
//!
//! 1. Writes hook wrapper scripts at
//!    `<workspace>/.claude/hooks/<repo>--<name>.sh`. Each wrapper reads
//!    the Claude Code tool-input JSON from stdin, extracts a target
//!    path (or falls back to `$PWD` for Bash calls), silently exits
//!    zero when the target is not under the repo's absolute path, and
//!    otherwise hands control to the real per-repo hook with the same
//!    stdin re-fed. Per-repo updates flow through automatically: the
//!    workspace wrapper is a thin scope check, the substantive logic
//!    still lives in `<repo>/.claude/hooks/<name>.sh`.
//!
//! 2. Merges per-repo `settings.json` hook registrations into the
//!    workspace `.claude/settings.json` with each command path
//!    rewritten to the workspace wrapper. Previously-aggregated entries
//!    (identified by `<repo>--` filename prefix matching any known
//!    workspace repo, or a legacy `imports/<repo>/` path prefix from
//!    the pre-homma bash aggregator) are filtered out before fresh
//!    entries land, so regens are idempotent and hand-authored
//!    workspace entries are preserved.
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

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use homma_api::{ContainedPath, Root};
use serde::Serialize;

/// One aggregated `hooks.PreToolUse` entry destined for the workspace
/// `settings.json`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HookEntry {
    pub matcher: String,
    pub command: String,
}

/// Aggregate one repo's hooks into the workspace `.claude/`. Returns
/// the number of hook wrappers written.
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
) -> Result<usize> {
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
    clean_stale(root, &ws_rules, repo_name, ".md")?;
    clean_stale(root, &ws_hooks, repo_name, ".sh")?;

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

    let hooks_count = aggregate_hooks(
        root,
        &claude_dir,
        &ws_hooks,
        repo_name,
        &repo_rel,
        settings_entries,
    )?;

    Ok(hooks_count)
}

/// Prove a workspace-relative path, naming what escaped when it does not.
fn contain(root: &Root, tail: &str) -> Result<ContainedPath> {
    root.contain(&root.as_abs().join(tail))
        .map_err(|e| anyhow!("{e}"))
}

/// Remove previously-aggregated files for `repo_name` so removed
/// per-repo entries do not linger at the workspace level.
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

/// Walk per-repo `.claude/hooks/`, write wrapper scripts to workspace
/// hooks dir, and collect settings.json registrations.
fn aggregate_hooks(
    root: &Root,
    repo_claude_dir: &Path,
    dst_dir: &ContainedPath,
    repo_name: &str,
    repo_rel_path: &str,
    settings_entries: &mut Vec<HookEntry>,
) -> Result<usize> {
    let src_dir = repo_claude_dir.join("hooks");
    if !src_dir.is_dir() {
        return Ok(0);
    }

    let per_repo_settings = read_settings_hooks(&repo_claude_dir.join("settings.json"));

    let mut count = 0;
    for entry in fs::read_dir(&src_dir).with_context(|| format!("read {}", src_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) if s.ends_with(".sh") => s.to_string(),
            _ => continue,
        };
        let stem_path = format!(".claude/hooks/{name}");
        let target_name = format!("{repo_name}--{name}");
        let target_path = root
            .contain_under(dst_dir, &target_name)
            .map_err(|e| anyhow!("{e}"))?;

        let wrapper = wrapper_script(repo_name, repo_rel_path, &stem_path);
        root.write(&target_path, wrapper)
            .with_context(|| format!("write {}", target_path.as_path().display()))?;
        #[cfg(unix)]
        root.set_executable(&target_path)?;

        let matchers = per_repo_settings.get(&stem_path);
        let matchers = match matchers {
            Some(m) if !m.is_empty() => m.clone(),
            _ => detect_matchers_from_hook_body(&path).unwrap_or_default(),
        };
        // `${CLAUDE_PROJECT_DIR}` rather than the path this run happened to
        // write to. The host substitutes it for the project root "regardless of
        // the working directory when the hook runs", which is what makes a
        // tracked `settings.json` name this workspace's wrappers in every
        // clone. The absolute form it replaces named the workspace that
        // generated the file, so every other clone either could not find the
        // command at all or, on the same machine, ran somebody else's copy.
        let command = format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{target_name}");
        for m in matchers {
            settings_entries.push(HookEntry {
                matcher: m,
                command: command.clone(),
            });
        }

        count += 1;
    }
    Ok(count)
}

/// Map a per-repo `settings.json`'s hook commands (`.claude/hooks/foo.sh`)
/// to the set of matchers each appears under across all PreToolUse
/// entries.
fn read_settings_hooks(path: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    use std::collections::BTreeMap;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return out,
    };
    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let events = match v.get("hooks").and_then(|h| h.as_object()) {
        Some(e) => e,
        None => return out,
    };
    for (_event_name, entries) in events {
        let arr = match entries.as_array() {
            Some(a) => a,
            None => continue,
        };
        for entry in arr {
            let matcher = match entry.get("matcher").and_then(|m| m.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let hooks = match entry.get("hooks").and_then(|h| h.as_array()) {
                Some(h) => h,
                None => continue,
            };
            for h in hooks {
                let cmd = match h.get("command").and_then(|c| c.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                out.entry(cmd).or_default().push(matcher.clone());
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
/// already carries as `repos.<name>.local_path`; `hook_rel_path` is the hook's
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
/// 3. Reads the tool-input JSON on stdin and extracts a target path (first
///    non-empty of `tool_input.file_path`, `tool_input.path`,
///    `tool_input.cwd`), falling back to `$PWD` for calls carrying no path
///    field.
/// 4. Exits 0 when the target is not under the repo root.
/// 5. Otherwise replaces itself with the real hook, re-feeding the original
///    stdin.
pub(crate) fn wrapper_script(repo_name: &str, repo_rel_path: &str, hook_rel_path: &str) -> String {
    let repo_rel = sh_single_quote_escape(repo_rel_path);
    let hook_rel = sh_single_quote_escape(hook_rel_path);
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

target=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // .tool_input.cwd // empty' 2>/dev/null)

if [ -z "$target" ]; then
    target="$PWD"
fi

case "$target" in
    "$REPO_ROOT"|"$REPO_ROOT"/*) ;;
    *) exit 0 ;;
esac

exec "$ORIG_HOOK" <<<"$INPUT"
"##
    )
}

/// Merge aggregated hook entries into the workspace `settings.json`,
/// preserving non-aggregated entries.
///
/// Two sets, and the difference between them is the whole of it.
/// `visited` names the repos this run actually aggregated, and their entries
/// are swept and rewritten. `known_repos` names every repo the manifest
/// declares, and is used only to recognise the legacy shape.
///
/// A repo the manifest declares but this workspace has not cloned aggregates
/// nothing, so sweeping on the full set deleted working registrations from
/// whichever workspace happened to run last. Its wrapper survives the sweep,
/// because the cleanup that would have removed it runs inside the per-repo
/// pass that was skipped, so the file and its registration ended up
/// disagreeing. Preserving those entries costs one `[ -x ]` in the wrapper
/// and makes the guard live the moment that repo is cloned.
///
/// The price, stated rather than discovered: a hook deleted upstream lingers
/// as a registration in every workspace that never clones its repo, pointing
/// at a wrapper that declines. That is the better side to be wrong on. The
/// alternative deletes working guards from every workspace holding a
/// different subset of the manifest, which is every workspace.
pub(crate) fn merge_settings(
    root: &Root,
    known_repos: &[&str],
    visited: &[&str],
    aggregated_entries: &[HookEntry],
    gate_entry: Option<&HookEntry>,
) -> Result<()> {
    let settings_path = contain(root, ".claude/settings.json")?;
    root.create_dir_all(&contain(root, ".claude")?).ok();

    let mut value: serde_json::Value = match fs::read_to_string(settings_path.as_path()) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s)
                .with_context(|| format!("parsing {}", settings_path.as_path().display()))?
        },
        _ => serde_json::json!({}),
    };

    let hooks = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("settings.json root is not an object"))?
        .entry("hooks".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("settings.json `hooks` is not an object"))?;
    let pre = hooks_obj
        .entry("PreToolUse".to_string())
        .or_insert_with(|| serde_json::json!([]));
    let pre_arr = pre
        .as_array_mut()
        .ok_or_else(|| anyhow!("settings.json `hooks.PreToolUse` is not an array"))?;

    // Per-hook filtering. Earlier homma versions filtered per-entry via
    // `.any()`, which would drop an entire entry when any single hook in
    // its `hooks[]` array matched a managed pattern. That cost
    // hand-authored hooks bundled alongside aggregated ones. The
    // current shape walks each entry's hook array, drops only managed
    // hooks within it, and retains the entry when any non-managed
    // hooks remain. Side effect: entries with a missing or non-array
    // `hooks` field (malformed) now get swept instead of preserved.
    // The previous per-entry shape returned `false` on bad shape and
    // retained such entries; the per-hook shape's empty-array check
    // drops them. Preferable: malformed state should not be load-bearing.
    for entry in pre_arr.iter_mut() {
        if let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            hooks.retain(|h| {
                let cmd = h.get("command").and_then(|c| c.as_str()).unwrap_or("");
                !is_aggregated_command(cmd, visited)
                    && !is_legacy_aggregated_command(cmd, known_repos)
                    && !crate::cmd::gates::is_workspace_gate_command(cmd)
            });
        }
    }
    pre_arr.retain(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|a| !a.is_empty())
    });

    if let Some(g) = gate_entry {
        pre_arr.push(serde_json::json!({
            "matcher": g.matcher,
            "hooks": [
                { "type": "command", "command": g.command }
            ]
        }));
    }

    for e in aggregated_entries {
        pre_arr.push(serde_json::json!({
            "matcher": e.matcher,
            "hooks": [
                { "type": "command", "command": e.command }
            ]
        }));
    }

    let serialised = serde_json::to_string_pretty(&value)?;
    root.write(&settings_path, serialised + "\n")
        .with_context(|| format!("write {}", settings_path.as_path().display()))?;
    Ok(())
}

/// True if `entry` looks like a homma-aggregated hook entry. Two
/// patterns count:
///
/// - **Current homma shape**: the command path's basename (filename
///   after the last `/`) starts with `<known-repo>--`. Matches both
///   relative and absolute paths the current aggregator emits.
/// - **Legacy bash-aggregator shape**: the command path contains the
///   segment `imports/<known-repo>/`. The pre-homma bash script
///   organised its output under per-repo subdirectories; these
///   entries linger in workspace `settings.json` after the bash
///   aggregator retired. Detecting them here lets every regen sweep
///   them out idempotently.
/// True if a single hook command string looks aggregated. Used by
/// `merge_settings` to strip individual hooks within an entry.
pub(crate) fn is_aggregated_command(cmd: &str, repos: &[&str]) -> bool {
    let basename = cmd.rsplit('/').next().unwrap_or(cmd);
    repos
        .iter()
        .any(|repo| basename.starts_with(&format!("{repo}--")))
}

/// True if a hook command carries the retired bash aggregator's shape,
/// `imports/<repo>/...`. Swept on the full manifest rather than on the repos
/// this run visited: nothing writes this form any more, so an entry carrying
/// it is residue in every workspace and there is no clone where keeping it
/// would make it work again.
pub(crate) fn is_legacy_aggregated_command(cmd: &str, known_repos: &[&str]) -> bool {
    known_repos.iter().any(|repo| {
        let seg = format!("imports/{repo}/");
        cmd.contains(&format!("/{seg}")) || cmd.starts_with(&seg)
    })
}

#[cfg(test)]
mod tests {

    /// A `Root` over a test workspace, denying nothing that a test uses.
    ///
    /// The real code path, not a variant of it: these go through
    /// `Root::contain` exactly as production does, which is what makes the
    /// containment they assert mean anything.
    fn test_root(workspace: &Path) -> Root {
        Root::new(
            &homma_api::AbsPath::new(workspace).expect("a tempdir path is absolute"),
            homma_api::Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap()),
        )
        .expect("a tempdir is a legitimate root")
    }

    /// Mark a file executable, which the wrapper's own `[ -x ]` check reads.
    fn make_executable(p: &Path) {
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
        use std::io::Write;
        let payload = format!(r#"{{"tool_input":{{"file_path":"{}"}}}}"#, target.display());
        let mut child = std::process::Command::new("bash")
            .arg(wrapper)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
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

    #[test]
    fn a_repo_path_that_arrived_unanchored_still_scopes_the_wrapper() {
        // The shape a workspace whose path resolved to `"."` produces: the repo
        // reaches `aggregate_repo` as `./arvo` rather than as an absolute path,
        // because `resolve_local_path` gives up on a relative root and joins.
        // The old code carried that straight into the wrapper, where `/ws/./arvo`
        // is not a textual prefix of `/ws/arvo/src/lib.rs`, so the scope check
        // never fired and every path exited 0.
        //
        // The path is built with `PathBuf::from` rather than `Path::join`,
        // deliberately. An earlier draft joined `"./arvo"` onto the workspace
        // and was insensitive to the whole defect, because `Path::components`
        // drops a `.` unprompted and the fixture never carried one. Its
        // negative control is what said so.
        let ws = tempfile::tempdir().unwrap();
        let repo = ws.path().join("arvo");
        fs::create_dir_all(repo.join(".claude/hooks")).unwrap();

        let marker = ws.path().join("fired");
        let real = repo.join(".claude/hooks/foo.sh");
        fs::write(
            &real,
            format!(
                "#!/usr/bin/env bash\ncat > /dev/null\ntouch '{}'\n",
                marker.display()
            ),
        )
        .unwrap();
        make_executable(&real);
        fs::write(
            repo.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/foo.sh"}]}]}}"#,
        )
        .unwrap();

        // The unanchored form, verbatim. `strip_prefix` cannot remove an
        // absolute workspace root from it, so it falls through unchanged and is
        // exactly what the emitted wrapper would carry.
        let declared = std::path::PathBuf::from(format!("{}/./arvo", ws.path().display()));
        assert!(
            declared.as_os_str().to_string_lossy().contains("/./"),
            "the fixture lost the component it exists to carry"
        );

        let mut entries = Vec::new();
        aggregate_repo(&test_root(ws.path()), "arvo", &declared, &mut entries).unwrap();

        let wrapper = ws.path().join(".claude/hooks/arvo--foo.sh");
        make_executable(&wrapper);
        let body = fs::read_to_string(&wrapper).unwrap();
        assert!(
            !body.contains("/./"),
            "the emitted wrapper carries a no-op path component:\n{body}"
        );

        run_wrapper(&wrapper, &repo.join("src/lib.rs"));
        assert!(
            marker.exists(),
            "the wrapper did not hand off for a path inside a repo that arrived unanchored"
        );

        // The control, so the assertion above is not satisfied by a wrapper
        // that hands off unconditionally.
        fs::remove_file(&marker).unwrap();
        run_wrapper(&wrapper, &ws.path().join("kolli/src/lib.rs"));
        assert!(
            !marker.exists(),
            "the wrapper handed off for a path outside the repo"
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
        assert!(is_legacy_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
        // and the current-shape check does not claim it, so the two are not
        // silently the same predicate under two names
        assert!(!is_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
    }

    #[test]
    fn legacy_imports_at_path_start_detected_as_aggregated() {
        // Relative path starting with `imports/<repo>/` (no leading
        // separator). Must still match the legacy pattern.
        let cmd = "imports/arvo/no-alloc-guard.sh";
        assert!(is_legacy_aggregated_command(cmd, &["arvo"]));
    }

    #[test]
    fn imports_substring_not_at_path_boundary_not_detected() {
        // A command that happens to contain the substring `imports/arvo/`
        // in the middle of a longer path component must NOT be flagged.
        // Path-component anchoring prevents false positives on
        // e.g. user-authored paths like `myimports/arvo/foo.sh`.
        let cmd = ".claude/hooks/myimports/arvo/foo.sh";
        assert!(!is_legacy_aggregated_command(cmd, &["arvo"]));
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
        fs::write(
            repo_abs.join(".claude/settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/no-alloc.sh"}]}]}}"#,
        )
        .unwrap();

        let mut settings = Vec::new();
        let h = aggregate_repo(&test_root(workspace), "arvo", &repo_abs, &mut settings).unwrap();
        assert_eq!(h, 1);

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
                {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--old.sh"}]}
            ]}}"#,
        )
        .unwrap();

        let entries = vec![HookEntry {
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
