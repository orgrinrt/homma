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

use anyhow::{anyhow, Context, Result};
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
    workspace: &Path,
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

    let ws_rules = workspace.join(".claude/rules");
    let ws_hooks = workspace.join(".claude/hooks");
    fs::create_dir_all(&ws_rules).ok();
    fs::create_dir_all(&ws_hooks).ok();

    // Cleans both prior-homma aggregated rules (kept after retirement
    // of rule aggregation) and prior-regen hook wrappers (the
    // idempotency guarantee for hooks).
    clean_stale(&ws_rules, repo_name, ".md")?;
    clean_stale(&ws_hooks, repo_name, ".sh")?;

    let hooks_count = aggregate_hooks(
        &claude_dir,
        &ws_hooks,
        repo_name,
        repo_abs_path,
        settings_entries,
    )?;

    Ok(hooks_count)
}

/// Remove previously-aggregated files for `repo_name` so removed
/// per-repo entries do not linger at the workspace level.
fn clean_stale(dir: &Path, repo_name: &str, ext: &str) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let prefix = format!("{repo_name}--");
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.starts_with(&prefix) && s.ends_with(ext) {
            fs::remove_file(entry.path()).ok();
        }
    }
    Ok(())
}

/// Walk per-repo `.claude/hooks/`, write wrapper scripts to workspace
/// hooks dir, and collect settings.json registrations.
fn aggregate_hooks(
    repo_claude_dir: &Path,
    dst_dir: &Path,
    repo_name: &str,
    repo_abs_path: &Path,
    settings_entries: &mut Vec<HookEntry>,
) -> Result<usize> {
    let src_dir = repo_claude_dir.join("hooks");
    if !src_dir.is_dir() {
        return Ok(0);
    }

    let per_repo_settings = read_settings_hooks(&repo_claude_dir.join("settings.json"));

    let mut count = 0;
    for entry in fs::read_dir(&src_dir).with_context(|| format!("read {}", src_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) if s.ends_with(".sh") => s.to_string(),
            _ => continue,
        };
        let stem_path = format!(".claude/hooks/{name}");
        let target_name = format!("{repo_name}--{name}");
        let target_path = dst_dir.join(&target_name);

        let orig_abs = repo_abs_path.join(".claude/hooks").join(&name);
        let repo_abs_str = repo_abs_path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 path: {}", repo_abs_path.display()))?;
        let orig_abs_str = orig_abs
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 path: {}", orig_abs.display()))?;

        let wrapper = wrapper_script(repo_name, repo_abs_str, orig_abs_str);
        fs::write(&target_path, wrapper)
            .with_context(|| format!("write {}", target_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&target_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&target_path, perms)?;
        }

        let matchers = per_repo_settings.get(&stem_path);
        let matchers = match matchers {
            Some(m) if !m.is_empty() => m.clone(),
            _ => detect_matchers_from_hook_body(&path).unwrap_or_default(),
        };
        let abs_command = target_path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 path: {}", target_path.display()))?
            .to_string();
        for m in matchers {
            settings_entries.push(HookEntry {
                matcher: m,
                command: abs_command.clone(),
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
/// The wrapper:
/// 1. Reads the Claude Code tool-input JSON on stdin.
/// 2. Extracts a target path (first non-empty of `tool_input.file_path`,
///    `tool_input.path`, `tool_input.cwd`); for Bash calls with no path
///    field, falls back to `$PWD`.
/// 3. Exits 0 silently when the target is not under the repo's
///    absolute path (default "allow", no decision emitted).
/// 4. Otherwise, replaces the current process with the real per-repo
///    hook, re-feeding the original stdin via a here-string.
pub(crate) fn wrapper_script(
    repo_name: &str,
    repo_abs_path: &str,
    orig_hook_abs_path: &str,
) -> String {
    let repo_root = sh_single_quote_escape(repo_abs_path);
    let orig_hook = sh_single_quote_escape(orig_hook_abs_path);
    format!(
        r##"#!/usr/bin/env bash
# Aggregated from `{repo_name}` by `homma agent regen`.
# Scoped to {repo_abs_path}.
# Source hook: {orig_hook_abs_path}

set -u

REPO_ROOT='{repo_root}'
ORIG_HOOK='{orig_hook}'

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
/// Identifies previously-aggregated entries by command path matching
/// `.claude/hooks/<known-repo>--*.sh`; those are filtered out, then
/// fresh `aggregated_entries` are appended.
pub(crate) fn merge_settings(
    workspace: &Path,
    known_repos: &[&str],
    aggregated_entries: &[HookEntry],
    gate_entry: Option<&HookEntry>,
) -> Result<()> {
    let settings_path = workspace.join(".claude/settings.json");
    fs::create_dir_all(workspace.join(".claude")).ok();

    let mut value: serde_json::Value = match fs::read_to_string(&settings_path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s)
            .with_context(|| format!("parsing {}", settings_path.display()))?,
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
        if let Some(hooks) = entry
            .get_mut("hooks")
            .and_then(|h| h.as_array_mut())
        {
            hooks.retain(|h| {
                let cmd = h
                    .get("command")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                !is_aggregated_command(cmd, known_repos)
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
    fs::write(&settings_path, serialised + "\n")
        .with_context(|| format!("write {}", settings_path.display()))?;
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
pub(crate) fn is_aggregated_entry(
    entry: &serde_json::Value,
    known_repos: &[&str],
) -> bool {
    let hooks = match entry.get("hooks").and_then(|h| h.as_array()) {
        Some(h) => h,
        None => return false,
    };
    hooks.iter().any(|h| {
        let cmd = h
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        is_aggregated_command(cmd, known_repos)
    })
}

/// True if a single hook command string looks aggregated. The per-hook
/// flavour of [`is_aggregated_entry`], used by `merge_settings` to
/// strip individual hooks without dropping the surrounding entry.
pub(crate) fn is_aggregated_command(cmd: &str, known_repos: &[&str]) -> bool {
    let basename = cmd.rsplit('/').next().unwrap_or(cmd);
    known_repos.iter().any(|repo| {
        let legacy_segment = format!("imports/{repo}/");
        basename.starts_with(&format!("{repo}--"))
            || cmd.contains(&format!("/{legacy_segment}"))
            || cmd.starts_with(&legacy_segment)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_script_contains_scope_check_and_handoff() {
        let s = wrapper_script("arvo", "/repos/arvo", "/repos/arvo/.claude/hooks/foo.sh");
        assert!(s.starts_with("#!/usr/bin/env bash"));
        assert!(s.contains("REPO_ROOT='/repos/arvo'"));
        assert!(s.contains("ORIG_HOOK='/repos/arvo/.claude/hooks/foo.sh'"));
        assert!(s.contains("$ORIG_HOOK"));
        assert!(s.contains("Aggregated from `arvo`"));
    }

    #[test]
    fn aggregated_entry_detected_by_command_prefix() {
        let entry = serde_json::json!({
            "matcher": "Edit",
            "hooks": [{ "type": "command", "command": ".claude/hooks/arvo--no-alloc-guard.sh" }]
        });
        assert!(is_aggregated_entry(&entry, &["arvo", "hilavitkutin"]));
    }

    #[test]
    fn non_aggregated_entry_not_detected() {
        let entry = serde_json::json!({
            "matcher": "Edit",
            "hooks": [{ "type": "command", "command": ".claude/hooks/workspace-only.sh" }]
        });
        assert!(!is_aggregated_entry(&entry, &["arvo"]));
    }

    #[test]
    fn legacy_imports_path_detected_as_aggregated() {
        // Pre-homma bash aggregator wrote per-repo hooks under
        // `imports/<repo>/<name>.sh` rather than the current flat
        // `<repo>--<name>.sh` convention. Entries left over from that
        // era must still get swept out on regen.
        let entry = serde_json::json!({
            "matcher": "Edit",
            "hooks": [{
                "type": "command",
                "command": ".claude/hooks/imports/arvo/no-alloc-guard.sh"
            }]
        });
        assert!(is_aggregated_entry(&entry, &["arvo", "hilavitkutin"]));
    }

    #[test]
    fn legacy_imports_at_path_start_detected_as_aggregated() {
        // Relative path starting with `imports/<repo>/` (no leading
        // separator). Must still match the legacy pattern.
        let entry = serde_json::json!({
            "matcher": "Edit",
            "hooks": [{
                "type": "command",
                "command": "imports/arvo/no-alloc-guard.sh"
            }]
        });
        assert!(is_aggregated_entry(&entry, &["arvo"]));
    }

    #[test]
    fn imports_substring_not_at_path_boundary_not_detected() {
        // A command that happens to contain the substring `imports/arvo/`
        // in the middle of a longer path component must NOT be flagged.
        // Path-component anchoring prevents false positives on
        // e.g. user-authored paths like `myimports/arvo/foo.sh`.
        let entry = serde_json::json!({
            "matcher": "Edit",
            "hooks": [{
                "type": "command",
                "command": ".claude/hooks/myimports/arvo/foo.sh"
            }]
        });
        assert!(!is_aggregated_entry(&entry, &["arvo"]));
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
        let h = aggregate_repo(workspace, "arvo", &repo_abs, &mut settings).unwrap();
        assert_eq!(h, 1);

        // Stale aggregated rule was cleaned.
        assert!(
            !workspace.join(".claude/rules/arvo--stale-rule.md").exists(),
            "stale aggregated rule should have been cleaned by clean_stale"
        );
        // Repo-side rule was NOT propagated.
        assert!(
            !workspace.join(".claude/rules/arvo--type-surface.md").exists(),
            "homma no longer aggregates per-repo rules"
        );

        let hook = fs::read_to_string(workspace.join(".claude/hooks/arvo--no-alloc.sh")).unwrap();
        assert!(hook.contains("REPO_ROOT='"));
        assert!(hook.contains("ORIG_HOOK='"));

        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].matcher, "Edit");
        assert!(
            settings[0].command.ends_with("/.claude/hooks/arvo--no-alloc.sh"),
            "expected absolute path ending with `.claude/hooks/arvo--no-alloc.sh`, got: {}",
            settings[0].command,
        );

        merge_settings(workspace, &["arvo"], &settings, None).unwrap();
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
        merge_settings(workspace, &["arvo"], &entries, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh"));
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--no-alloc.sh"));
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
        merge_settings(workspace, &["arvo"], &entries, None).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(!arr.iter().any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--old.sh"));
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == ".claude/hooks/workspace-byline.sh"));
        assert!(arr.iter().any(|e| e["hooks"][0]["command"] == ".claude/hooks/arvo--new.sh"));
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
        merge_settings(workspace, &["arvo"], &entries, None).unwrap();
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
        assert_eq!(edit_hooks.len(), 1, "aggregated hook should be stripped, hand-authored preserved");
        assert_eq!(
            edit_hooks[0]["command"], ".claude/hooks/workspace-handauthored.sh"
        );
        let write = arr.iter().find(|e| e["matcher"] == "Write").unwrap();
        assert_eq!(write["hooks"][0]["command"], ".claude/hooks/arvo--new.sh");
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

        merge_settings(workspace, &["arvo"], &[], None).unwrap();
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
