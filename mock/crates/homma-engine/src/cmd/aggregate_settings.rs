//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The settings half of aggregation: sweeping the registrations an older homma
//! wrote itself, under every event, out of `.claude/settings.local.json` and the
//! shared `.claude/settings.json`. Where nothing of homma's is in a file, it is not
//! written at all, so its bytes stay as somebody left them.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use homma_api::{ContainedPath, Root};

use super::{carries_the_mark, contain, hook_file_named};

/// The clone's own settings.
const LOCAL: &str = ".claude/settings.local.json";

/// The shared settings.
const SHARED: &str = ".claude/settings.json";

/// Sweep every registration homma wrote itself out of both settings files,
/// preserving everything else in both. homma registers nothing any more:
/// claude-shook registers its manifest, so a registration of homma's own would
/// only run a hook twice, and one naming a member this clone does not hold
/// brings nothing back.
///
/// A registration is swept only when it is homma's: a retired shape, or a
/// managed name whose file is absent or carries
/// [`MANAGED_MARK`](super::MANAGED_MARK). A file under a managed name that
/// homma did not write keeps its registration exactly as it is, in either
/// file. `known_repos` names every repo the manifest declares, and is used only
/// to recognise the retired shapes.
///
/// Both files are read and swept before either is written, so a file that does
/// not parse, or is not the shape a settings file has, leaves both as they
/// were. A file with nothing of homma's in it is not written at all, so its
/// bytes stay as somebody left them.
pub(crate) fn merge_settings(root: &Root, known_repos: &[&str]) -> Result<()> {
    let hooks_dir = root.as_abs().join(".claude/hooks");
    let is_ours = |cmd: &str| -> bool {
        if is_retired_aggregated_command(cmd, known_repos) {
            return true;
        }
        let managed = is_any_aggregated_command(cmd)
            || crate::cmd::gates::is_workspace_gate_command(cmd)
            || crate::cmd::declared::is_declared_command(cmd);
        managed && !names_a_file_homma_did_not_write(&hooks_dir, cmd)
    };

    let shared = contain(root, SHARED)?;
    let mut shared_value = read_settings(&shared)?;
    let shared_swept = match shared_value.as_mut() {
        Some(value) => sweep(value, SHARED, &is_ours)?,
        None => false,
    };
    let local = contain(root, LOCAL)?;
    let mut local_value = read_settings(&local)?;
    let local_swept = match local_value.as_mut() {
        Some(value) => sweep(value, LOCAL, &is_ours)?,
        None => false,
    };

    if let (true, Some(value)) = (local_swept, local_value.as_ref()) {
        write_settings(root, &local, value)?;
    }
    if let (true, Some(value)) = (shared_swept, shared_value.as_ref()) {
        write_settings(root, &shared, value)?;
    }
    Ok(())
}

/// True if a hook command runs a file of homma's aggregated shape,
/// `<repo>--<name>` under `.claude/hooks/`, whichever repo it names. The
/// workspace gate and the declared hooks are this shape too, under `_workspace`
/// and `_declared`, so this is every name homma gives a wrapper.
fn is_any_aggregated_command(cmd: &str) -> bool {
    hook_file_named(cmd).is_some_and(|name| {
        name.split_once("--")
            .is_some_and(|(repo, rest)| !repo.is_empty() && !rest.is_empty())
    })
}

/// A settings file parsed, or `None` when it is absent or holds only
/// whitespace, which the host reads as no settings at all.
fn read_settings(path: &ContainedPath) -> Result<Option<serde_json::Value>> {
    match fs::read_to_string(path.as_path()) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s)
                .map(Some)
                .with_context(|| format!("parsing {}", path.as_path().display()))
        },
        _ => Ok(None),
    }
}

fn write_settings(root: &Root, path: &ContainedPath, value: &serde_json::Value) -> Result<()> {
    let serialised = serde_json::to_string_pretty(value)?;
    root.write(path, serialised + "\n")
        .with_context(|| format!("write {}", path.as_path().display()))
}

/// Take every registration `is_ours` claims out of `value`, and say whether
/// anything went. A file with no `hooks` key is left without one.
///
/// Per hook rather than per entry, so a hand-written hook bundled in one entry
/// beside a managed one survives it. An entry is dropped only when this sweep
/// emptied it, and an event only when this sweep emptied its array: a malformed
/// entry, an empty one, or an empty event somebody wrote are not homma's to
/// remove.
fn sweep(
    value: &mut serde_json::Value,
    name: &str,
    is_ours: &dyn Fn(&str) -> bool,
) -> Result<bool> {
    let root_obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("{name} root is not an object"))?;
    let Some(hooks) = root_obj.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks_obj = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow!("{name} `hooks` is not an object"))?;

    let mut swept = false;
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
            swept |= hooks.len() < had;
            !(had > 0 && hooks.is_empty())
        });
        if before > 0 && arr.is_empty() {
            emptied.push(event.clone());
        }
    }
    for event in &emptied {
        hooks_obj.remove(event);
    }
    Ok(swept)
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
