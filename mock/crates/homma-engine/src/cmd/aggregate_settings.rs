//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The workspace `settings.json` half of aggregation: sweeping the
//! registrations homma wrote before and writing this run's, under every event.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use homma_api::Root;

use super::{HookEntry, carries_the_mark, contain, hook_file_named};

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
///
/// Every event is swept and written, not only `PreToolUse`, and a registration
/// is swept only when it is homma's: a retired shape, or a managed name whose
/// file is absent or carries [`MANAGED_MARK`](super::MANAGED_MARK). A file
/// under a managed name that homma did not write keeps its registration
/// exactly as it is.
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

    let hooks_dir = root.as_abs().join(".claude/hooks");
    let is_ours = |cmd: &str| -> bool {
        if is_retired_aggregated_command(cmd, known_repos) {
            return true;
        }
        let managed = is_aggregated_command(cmd, visited)
            || crate::cmd::gates::is_workspace_gate_command(cmd)
            || crate::cmd::declared::is_declared_command(cmd);
        managed && !names_a_file_homma_did_not_write(&hooks_dir, cmd)
    };

    // Per hook rather than per entry, so a hand-written hook bundled in one
    // entry beside a managed one survives it. An entry is dropped only when
    // this sweep emptied it, and an event only when this sweep emptied its
    // array: a malformed entry, an empty one, or an empty event somebody wrote
    // are not homma's to remove.
    let mut emptied: Vec<String> = Vec::new();
    for (event, entries) in hooks_obj.iter_mut() {
        let Some(arr) = entries.as_array_mut() else {
            continue;
        };
        let before = arr.len();
        arr.retain_mut(|entry| {
            let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                return true;
            };
            let had = hooks.len();
            hooks.retain(|h| !is_ours(h.get("command").and_then(|c| c.as_str()).unwrap_or("")));
            !(had > 0 && hooks.is_empty())
        });
        if before > 0 && arr.is_empty() {
            emptied.push(event.clone());
        }
    }
    for event in &emptied {
        hooks_obj.remove(event);
    }

    for e in gate_entry.into_iter().chain(aggregated_entries) {
        hooks_obj
            .entry(e.event.clone())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| anyhow!("settings.json `hooks.{}` is not an array", e.event))?
            .push(e.to_json());
    }

    let serialised = serde_json::to_string_pretty(&value)?;
    root.write(&settings_path, serialised + "\n")
        .with_context(|| format!("write {}", settings_path.as_path().display()))?;
    Ok(())
}

/// Whether `cmd` runs a file under the workspace's `.claude/hooks/` that is
/// there and does not carry the mark: a hand-written hook under a name homma
/// also uses, whose registration is kept exactly as it is.
fn names_a_file_homma_did_not_write(hooks_dir: &Path, cmd: &str) -> bool {
    hook_file_named(cmd).is_some_and(|name| {
        let p = hooks_dir.join(name);
        p.exists() && !carries_the_mark(&p)
    })
}

/// True if a single hook command string looks aggregated: the command path's
/// basename, after the last `/`, starts with `<known-repo>--`, which is the
/// shape the aggregator emits whether the path is relative or absolute. Used
/// by `merge_settings` to strip individual hooks within an entry, and by
/// `is_retired_aggregated_command`, which owns the shapes nothing writes any
/// more: the retired bash aggregator's `imports/<known-repo>/`, and a managed
/// command naming an absolute path.
pub(crate) fn is_aggregated_command(cmd: &str, repos: &[&str]) -> bool {
    let basename = cmd.rsplit('/').next().unwrap_or(cmd);
    repos
        .iter()
        .any(|repo| basename.starts_with(&format!("{repo}--")))
}

/// True if a managed hook command carries a shape nothing writes any more.
/// Swept on the full manifest rather than on the repos this run visited,
/// because there is no clone in which keeping one would make it work again.
///
/// Two shapes qualify. The retired bash aggregator's `imports/<repo>/...`,
/// which the current aggregator replaced with a flat `<repo>--<name>.sh`. And a
/// managed command naming an **absolute** path, which is what this aggregator
/// itself wrote before it learned to name `${CLAUDE_PROJECT_DIR}`.
///
/// That absolute path belongs to whichever workspace generated it, and cloning
/// the repo does not make it resolve here. That is exactly what separates it
/// from a placeholder command for an unvisited repo, which does come back to
/// life and is preserved. Left in place it is worse than inert: on the machine
/// that generated it the path exists, so the host runs another workspace's
/// hooks against this one's edits.
///
/// A command that is not managed is untouched by either arm. A hand-authored
/// user-level hook is absolute too, and its basename matches no repo.
pub(crate) fn is_retired_aggregated_command(cmd: &str, known_repos: &[&str]) -> bool {
    let legacy = known_repos.iter().any(|repo| {
        let seg = format!("imports/{repo}/");
        cmd.contains(&format!("/{seg}")) || cmd.starts_with(&seg)
    });
    legacy || (cmd.starts_with('/') && is_aggregated_command(cmd, known_repos))
}

#[cfg(test)]
#[path = "aggregate_settings_tests.rs"]
mod tests;
