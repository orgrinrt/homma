//! Workspace-level aggregation of per-repo mockspace agent surfaces.
//!
//! For each member repo with rendered `.claude/rules/*.md` and
//! `.claude/hooks/*.sh`, this module:
//!
//! 1. Copies rules into `<workspace>/.claude/rules/<repo>--<name>.md`,
//!    rewriting the `paths:` front matter field so the rule only loads
//!    when files in that repo's tree are touched. Existing `paths:`
//!    globs get the repo's local path prepended; absent `paths:` blocks
//!    get one injected matching `<local_path>/**`.
//!
//! 2. Writes hook wrapper scripts at
//!    `<workspace>/.claude/hooks/<repo>--<name>.sh`. Each wrapper reads
//!    the Claude Code tool-input JSON from stdin, extracts a target
//!    path (or falls back to `$PWD` for Bash calls), silently exits
//!    zero when the target is not under the repo's absolute path, and
//!    otherwise hands control to the real per-repo hook with the same
//!    stdin re-fed. Per-repo updates flow through automatically: the
//!    workspace wrapper is a thin scope check, the substantive logic
//!    still lives in `<repo>/.claude/hooks/<name>.sh`.
//!
//! 3. Merges per-repo `settings.json` hook registrations into the
//!    workspace `.claude/settings.json` with each command path
//!    rewritten to the workspace wrapper. Previously-aggregated entries
//!    (identified by `<repo>--` filename prefix matching any known
//!    workspace repo) are filtered out before fresh entries land, so
//!    regens are idempotent and hand-authored workspace entries are
//!    preserved.
//!
//! Skills are deliberately NOT aggregated. Claude Code skills do not
//! support `paths:`-scoped activation, so workspace-level skills would
//! load unconditionally and the scoping property would not hold.

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

/// Aggregate one repo's rules + hooks into the workspace `.claude/`.
/// Returns `(rules_written, hooks_written)`.
///
/// `settings_entries` accumulates per-hook registrations that
/// [`merge_settings`] writes into the workspace `settings.json` after
/// the per-repo loop completes.
pub(crate) fn aggregate_repo(
    workspace: &Path,
    repo_name: &str,
    repo_local_path: &Path,
    repo_abs_path: &Path,
    settings_entries: &mut Vec<HookEntry>,
) -> Result<(usize, usize)> {
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

    clean_stale(&ws_rules, repo_name, ".md")?;
    clean_stale(&ws_hooks, repo_name, ".sh")?;

    let rules_count = aggregate_rules(
        &claude_dir.join("rules"),
        &ws_rules,
        repo_name,
        repo_local_path,
    )?;
    let hooks_count = aggregate_hooks(
        &claude_dir,
        &ws_hooks,
        repo_name,
        repo_abs_path,
        settings_entries,
    )?;

    Ok((rules_count, hooks_count))
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

/// Walk per-repo `.claude/rules/`, rewrite each rule's front-matter
/// `paths:` field, write to workspace rules dir.
fn aggregate_rules(
    src_dir: &Path,
    dst_dir: &Path,
    repo_name: &str,
    repo_local_path: &Path,
) -> Result<usize> {
    if !src_dir.is_dir() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(src_dir).with_context(|| format!("read {}", src_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) if s.ends_with(".md") => s.to_string(),
            _ => continue,
        };
        let content = fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let rewritten = rewrite_rule_front_matter(&content, repo_local_path);
        let target = dst_dir.join(format!("{repo_name}--{name}"));
        fs::write(&target, rewritten)
            .with_context(|| format!("write {}", target.display()))?;
        count += 1;
    }
    Ok(count)
}

/// Rewrite the `paths:` block in a rule's YAML-like front matter to
/// prepend `<repo_local_path>/` to each glob. If no front matter is
/// present, prepend a fresh one containing `<repo_local_path>/**`. If
/// front matter is present but lacks a `paths:` block, append one.
///
/// Mockspace emits a reliably simple `paths:` block (`paths:` followed
/// by `  - "..."` lines), so a line-oriented rewrite is enough; no
/// full YAML parser needed at this layer.
pub(crate) fn rewrite_rule_front_matter(content: &str, repo_local_path: &Path) -> String {
    let local = repo_local_path.to_string_lossy();
    let local = local.trim_end_matches('/');
    let blanket = format!("{local}/**");

    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            let front = &rest[..end];
            let body = &rest[end + 5..];
            let new_front = rewrite_front_matter_paths(front, local, &blanket);
            return format!("---\n{new_front}\n---\n{body}");
        }
    }

    format!("---\npaths:\n  - \"{blanket}\"\n---\n{content}")
}

/// Line-oriented rewriter for the `paths:` block inside YAML-like
/// front matter. Lines outside the block pass through unchanged. The
/// block is identified by a `paths:` key at indent zero; continuation
/// lines start with `- ` (after optional whitespace).
fn rewrite_front_matter_paths(front: &str, local: &str, blanket: &str) -> String {
    let mut out = String::new();
    let mut saw_paths = false;
    let mut in_paths_block = false;

    for line in front.lines() {
        if in_paths_block {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- ") || trimmed.starts_with("-\"") {
                let value_part = trimmed.trim_start_matches('-').trim_start();
                let value = value_part
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                out.push_str("  - \"");
                out.push_str(local);
                out.push('/');
                out.push_str(value);
                out.push_str("\"\n");
                continue;
            } else {
                in_paths_block = false;
            }
        }

        if line.trim_start().starts_with("paths:") {
            saw_paths = true;
            in_paths_block = true;
            out.push_str("paths:\n");
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    if !saw_paths {
        out.push_str("paths:\n  - \"");
        out.push_str(blanket);
        out.push_str("\"\n");
    }

    out.trim_end_matches('\n').to_string()
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
        for m in matchers {
            settings_entries.push(HookEntry {
                matcher: m,
                command: format!(".claude/hooks/{target_name}"),
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
    format!(
        r##"#!/usr/bin/env bash
# Aggregated from `{repo_name}` by `homma agent regen`.
# Scoped to {repo_abs_path}.
# Source hook: {orig_hook_abs_path}

set -u

REPO_ROOT='{repo_abs_path}'
ORIG_HOOK='{orig_hook_abs_path}'

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

    pre_arr.retain(|entry| {
        !is_aggregated_entry(entry, known_repos)
            && !crate::cmd::gates::is_workspace_gate_entry(entry)
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

/// True if `entry` looks like a homma-aggregated hook entry: its
/// command path stripped of `.claude/hooks/` matches `<known-repo>--`.
pub(crate) fn is_aggregated_entry(
    entry: &serde_json::Value,
    known_repos: &[&str],
) -> bool {
    let hooks = match entry.get("hooks").and_then(|h| h.as_array()) {
        Some(h) => h,
        None => return false,
    };
    hooks.iter().any(|h| {
        let cmd = match h.get("command").and_then(|c| c.as_str()) {
            Some(s) => s,
            None => return false,
        };
        let stripped = match cmd.strip_prefix(".claude/hooks/") {
            Some(s) => s,
            None => return false,
        };
        known_repos
            .iter()
            .any(|repo| stripped.starts_with(&format!("{repo}--")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_inserts_paths_when_absent() {
        let content = "# Some rule\n\nContents.\n";
        let out = rewrite_rule_front_matter(content, Path::new("arvo"));
        assert!(out.starts_with("---\npaths:\n  - \"arvo/**\"\n---\n"));
        assert!(out.contains("Contents."));
    }

    #[test]
    fn rewrite_prepends_repo_path_to_existing_paths() {
        let content = "---\npaths:\n  - \"crates/**/*.rs\"\n  - \"src/**/*.rs\"\n---\nBody.\n";
        let out = rewrite_rule_front_matter(content, Path::new("arvo"));
        assert!(out.contains("- \"arvo/crates/**/*.rs\""), "got: {out}");
        assert!(out.contains("- \"arvo/src/**/*.rs\""), "got: {out}");
        assert!(out.contains("Body."));
    }

    #[test]
    fn rewrite_preserves_other_front_matter_fields() {
        let content = "---\ntitle: foo\npaths:\n  - \"x.rs\"\nname: bar\n---\nBody.\n";
        let out = rewrite_rule_front_matter(content, Path::new("arvo"));
        assert!(out.contains("title: foo"), "got: {out}");
        assert!(out.contains("name: bar"), "got: {out}");
        assert!(out.contains("- \"arvo/x.rs\""), "got: {out}");
    }

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
        let repo_local = Path::new("arvo");
        let repo_abs = workspace.join(repo_local);
        fs::create_dir_all(repo_abs.join(".claude/rules")).unwrap();
        fs::create_dir_all(repo_abs.join(".claude/hooks")).unwrap();

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
        let (r, h) = aggregate_repo(workspace, "arvo", repo_local, &repo_abs, &mut settings).unwrap();
        assert_eq!(r, 1);
        assert_eq!(h, 1);

        let rule = fs::read_to_string(workspace.join(".claude/rules/arvo--type-surface.md")).unwrap();
        assert!(rule.contains("- \"arvo/crates/**/*.rs\""), "got: {rule}");

        let hook = fs::read_to_string(workspace.join(".claude/hooks/arvo--no-alloc.sh")).unwrap();
        assert!(hook.contains("REPO_ROOT='"));
        assert!(hook.contains("ORIG_HOOK='"));

        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].matcher, "Edit");
        assert_eq!(settings[0].command, ".claude/hooks/arvo--no-alloc.sh");

        merge_settings(workspace, &["arvo"], &settings, None).unwrap();
        let written = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&written).unwrap();
        let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["matcher"], "Edit");
        assert_eq!(arr[0]["hooks"][0]["command"], ".claude/hooks/arvo--no-alloc.sh");
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
}
