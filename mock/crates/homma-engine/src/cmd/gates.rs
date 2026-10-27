//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Workspace-level git-ops validation gate.
//!
//! Generates a workspace `.claude/hooks/` shell script
//! that intercepts Bash tool calls running `git commit` or `git push`
//! from within a member repo's subtree and verifies that repo has a
//! healthy mockspace bootstrap before allowing the operation through.
//!
//! The gate is workspace-authored (not aggregated): one script registered
//! against the Bash matcher, embedded with the list of member-repo
//! absolute paths at generation time. When fired, it:
//!
//! 1. Reads the Claude Code tool-input JSON from stdin.
//! 2. Checks the `tool_input.command` field; exits 0 silently if it does
//!    not look like `git commit` or `git push` (out-of-scope call).
//! 3. Resolves the active repo from `tool_input.cwd` (or `$PWD` as
//!    fallback); exits 0 silently if no member repo's tree covers it.
//! 4. Asks `homma agent status` about the repo, and skips the drift check if
//!    it has no `mockspace.toml`, which is what adopting mockspace is.
//!    Otherwise it verifies the mock directory the toml declares, the
//!    launcher pin in `mockspace.toml`, and a `core.hooksPath` set so
//!    per-repo git hooks fire. It holds no reader of the toml of its own, and
//!    refuses where it cannot ask.
//! 5. If healthy: silent allow. If anything is missing: structured deny
//!    with a hookSpecificOutput payload pointing at the fix.
//!
//! Defence-in-depth: per-repo git hooks (installed by mockspace
//! bootstrap) are the primary enforcement. This workspace gate covers
//! the case where the per-repo hook would not fire because the bootstrap
//! was never run or `core.hooksPath` got reset.

use anyhow::{Context, Result};

use crate::cmd::aggregate::HookEntry;

/// File name for the generated workspace gate script.
const GATE_SCRIPT_NAME: &str = "_workspace--mockspace-gate.sh";

/// Generate the workspace mockspace gate script and return the
/// `settings.local.json` entry registering it.
///
/// The script is written to `<workspace>/.claude/hooks/`. The returned
/// entry is appended to `hooks.PreToolUse[]` with matcher `Bash` so it
/// fires on every Bash tool call; the script itself decides which
/// commands warrant inspection.
///
/// `repos` is the list of `(repo_name, workspace_relative_path)` pairs that
/// the gate should consider as "member repos" worth validating. Relative,
/// because a path naming where the clone sat when it generated stops matching
/// the moment the clone is moved or renamed. A repo declared outside
/// the workspace has no relative form and keeps its absolute one; the script
/// handles both.
pub(crate) fn install_workspace_gate(
    root: &homma_api::Root,
    repos: &[(String, String)],
) -> Result<HookEntry> {
    // Proven, like every other write this pass performs. This one installs an
    // executable the agent harness then runs on every tool call, which is the
    // write with the largest consequence in the program.
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    root.create_dir_all(&hooks_dir).ok();
    let target = root
        .contain_under(&hooks_dir, GATE_SCRIPT_NAME)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // Normalised here rather than by the caller, because a caller that
    // forgets is a caller nothing can catch: the resulting table entry is a
    // plausible-looking string that simply never matches, and the gate then
    // finds no repo covering any path. The function that owns the contract is
    // the one place a test can hold it.
    let repos: Vec<(String, String)> = repos
        .iter()
        .map(|(n, p)| {
            (
                n.clone(),
                crate::cmd::util::relative_str(std::path::Path::new(p)),
            )
        })
        .collect();
    // A file under the gate's name that homma did not write is somebody's, and
    // is refused by path rather than replaced.
    if target.as_path().exists() && !crate::cmd::aggregate::carries_the_mark(target.as_path()) {
        return Err(anyhow::anyhow!(
            "`.claude/hooks/{GATE_SCRIPT_NAME}` was not written by homma, so the workspace gate \
             is not written over it; move that file aside or rename it"
        ));
    }
    let body = gate_script(&repos);
    root.write(&target, body)
        .with_context(|| format!("writing {}", target.as_path().display()))?;
    #[cfg(unix)]
    root.set_executable(&target)?;
    // `${CLAUDE_PROJECT_DIR}`, for the same reason the per-repo wrappers use
    // it: the host substitutes the project root regardless of the working
    // directory the hook runs in, so the registration stays right when the
    // clone is moved or copied.
    Ok(HookEntry {
        event:   "PreToolUse".to_string(),
        matcher: "Bash".to_string(),
        command: format!("\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{GATE_SCRIPT_NAME}"),
    })
}

/// True if a single hook command string points at the workspace-gate
/// script. Used by `merge_settings` when filtering hooks individually
/// within a multi-hook entry. Identification is by command-path
/// basename so absolute and relative path forms both match cleanly
/// across regens.
pub(crate) fn is_workspace_gate_command(cmd: &str) -> bool {
    cmd.rsplit('/').next().unwrap_or(cmd) == GATE_SCRIPT_NAME
}

/// Render the gate script body with the workspace's member-repo list
/// baked in. The script is portable bash (>=3.2, the macOS default).
fn gate_script(repos: &[(String, String)]) -> String {
    let mut repo_table = String::new();
    for (name, path) in repos {
        // Escape any embedded single quotes so the bash literal stays
        // well-formed for paths or names containing them.
        let entry = crate::cmd::aggregate::sh_single_quote_escape(&format!("{name}|{path}"));
        repo_table.push_str(&format!("    '{entry}'\n"));
    }

    format!(
        r##"#!/usr/bin/env bash
# Generated by `homma agent regen` (#454 phase 3).
# Workspace-level mockspace bootstrap gate.
#
# Intercepts Bash tool calls running git commit / git push from inside a
# member repo's tree and denies the call if the repo's mockspace
# bootstrap is missing or broken. Per-repo git hooks remain the primary
# enforcement; this gate covers the case where they would not fire.

set -u

# The workspace this file sits in, found from its own location: a gate script
# lives at `<workspace>/.claude/hooks/`, a fixed depth, so it needs no baked
# prefix and no environment variable to know where the member repos are.
WS=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")/../.." && pwd) || exit 0

INPUT=$(cat)

# `jq` reads the input below and encodes the reason at the bottom, so without it
# this script cannot tell an out-of-scope call from one it should stop.
#
# **It said nothing and allowed everything.** Both reads carried `2>/dev/null`,
# so a missing `jq` produced an empty command line, the out-of-scope branch
# below took it, and every commit and every push went through at exit 0. That is
# indistinguishable from the gate having looked and found nothing wrong, which
# is the one thing a gate may never be.
#
# So it denies instead. A denial is visible and is one `brew install jq` from
# gone; a silent allow is neither.
if ! command -v jq >/dev/null 2>&1; then
    printf '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}}}\n' \
        "the mockspace adoption gate needs jq and cannot find it on PATH, so it cannot read this call. Install jq, or remove this hook."
    exit 0
fi

command_line=$(printf '%s' "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null)

# Out of scope: not a Bash command call.
if [ -z "$command_line" ]; then
    exit 0
fi

# Out of scope: command does not start with `git commit` or `git push`.
# Tolerate a leading subdir cd (`cd foo && git commit ...`).
case "$command_line" in
    *"git commit"*) ;;
    *"git push"*) ;;
    *) exit 0 ;;
esac

# Determine active directory: prefer tool_input.cwd, fall back to PWD.
target_dir=$(printf '%s' "$INPUT" | jq -r '.tool_input.cwd // empty' 2>/dev/null)
if [ -z "$target_dir" ]; then
    target_dir="$PWD"
fi

# Member-repo table: each entry is `<name>|<path>`, where the path is
# relative to the workspace unless the manifest declared the repo outside it.
REPOS=(
{repo_table})

# Find which repo's tree covers target_dir (longest-prefix match).
match_name=""
match_path=""
for entry in "${{REPOS[@]}}"; do
    name="${{entry%%|*}}"
    raw="${{entry#*|}}"
    case "$raw" in
        /*) path="$raw" ;;
        *)  path="$WS/$raw" ;;
    esac
    case "$target_dir" in
        "$path"|"$path"/*)
            # Prefer the most specific (longest) path.
            if [ "${{#path}}" -gt "${{#match_path}}" ]; then
                match_name="$name"
                match_path="$path"
            fi
            ;;
    esac
done

# Out of scope: target is not under any member repo.
if [ -z "$match_path" ]; then
    exit 0
fi

# The mockspace tool repo is not a mockspace *consumer*. Its mock/ is v2
# self-hosting and its v1 src is intentionally ungated (no core.hooksPath),
# so it always scores partial adoption by design, not drift. Skip the gate
# for it, identified by its own Cargo package name.
if [ "$match_name" = "mockspace" ] \
    && grep -qE '^name[[:space:]]*=[[:space:]]*"mockspace"[[:space:]]*$' "$match_path/Cargo.toml" 2>/dev/null; then
    exit 0
fi
# Which of mockspace's surfaces a repo has, and whether it adopted mockspace at
# all, which is whether it carries a `mockspace.toml`. A repo without the toml
# has not adopted mockspace, whatever else it shows, and the command the drift
# deny would name has nothing to run there, so it is not checked for drift and
# only the shared config check below applies to it. Of one that has the toml,
# some surfaces present and others missing is drift.
#
# What the toml says is asked of homma and never read here: whether it is
# there, the directory it declares as `mock_dir` (`mock` when it declares none),
# whether that directory exists, and whether it carries a launcher pin. homma
# reads the file as a toml. A second reader in awk disagreed with it on valid
# files each time one was patched toward the other, so there is one.
#
# No manifest means no workspace: the repo table this script carries came from
# one, and without it there is nothing for homma to resolve `$match_name`
# against. Skipping is the honest answer rather than refusing, because a
# refusal would be this gate reporting the absence of its own input as a fault
# in somebody's repo.
#
# It refuses when it cannot ask at all, which is what this script already does
# for a missing `jq`, for the reason argued there: a check that could not run
# must never be indistinguishable from one that ran and found nothing.
mock_dir_name=mock
has_toml=false
has_mock_dir=false
has_pin=false
has_hooks_path=0

problems=()
fix=""

if [ -f "$WS/homma.toml" ]; then
    if ! command -v homma >/dev/null 2>&1; then
        problems+=("homma is not on PATH, so the mockspace surfaces and the shared tool configs could not be checked")
    else
        status_out=$(homma --dir "$WS" agent status --repo "$match_name" --output json 2>&1)
        status_rc=$?
        facts=""
        if [ "$status_rc" -eq 0 ]; then
            facts=$(printf '%s' "$status_out" | jq -r '.repos[0].surfaces | [.mockspace_toml, .mock_dir_name, .mock_dir, .launcher_pin] | @tsv' 2>/dev/null)
        fi
        IFS=$'\t' read -r has_toml mock_dir_name has_mock_dir has_pin <<<"$facts"
        case "$has_toml$has_mock_dir$has_pin" in
            truetruetrue|truetruefalse|truefalsetrue|truefalsefalse|falsetruetrue|falsetruefalse|falsefalsetrue|falsefalsefalse) ;;
            *) status_rc=1 ;;
        esac

        if [ "$status_rc" -ne 0 ]; then
            problems+=("the mockspace surfaces could not be read: $status_out")
            fix="Read \`$WS/homma.toml\`; homma could not report on repo \`$match_name\`."
        elif [ "$has_toml" = "true" ]; then
            hooks_path="$(git -C "$match_path" config --get core.hooksPath 2>/dev/null)"
            if [ -n "$hooks_path" ]; then
                # The question this surface answers is whether mockspace's hooks
                # fire, and the path being set answered it only while mockspace
                # was the one thing that set it. git-shook points the same
                # setting at a dispatcher directory of its own, in a repository
                # that may never have heard of mockspace, and counting that
                # reads the repository as half-adopted and refuses every commit
                # in it with an instruction that cannot help.
                #
                # Compared against the directory git-shook generates rather
                # than matched by spelling, since a repository is free to name a
                # directory `shook` and mean nothing by it. git-shook writes the
                # path absolute and this resolves the same way, so the two agree
                # exactly or the path is somebody else's.
                common="$(git -C "$match_path" rev-parse --path-format=absolute \
                    --git-common-dir 2>/dev/null)"
                if [ "$hooks_path" != "$common/shook/hooks" ]; then
                    has_hooks_path=1
                fi
            fi

            adoption=0
            [ "$has_mock_dir" = "true" ] && adoption=$((adoption + 1))
            [ "$has_pin" = "true" ] && adoption=$((adoption + 1))
            [ "$has_hooks_path" -eq 1 ] && adoption=$((adoption + 1))

            # None of the surfaces is a repo that never finished opting in and
            # all of them is one with nothing to say. Anything between the two
            # is drift.
            if [ "$adoption" -ne 0 ] && [ "$adoption" -ne 3 ]; then
                regen_wanted=0
                if [ "$has_mock_dir" != "true" ]; then
                    problems+=("$match_path/$mock_dir_name/ is missing but other mockspace surfaces are present")
                    regen_wanted=1
                fi
                if [ "$has_pin" != "true" ]; then
                    problems+=("$match_path: no launcher pin (mockspace_* in mockspace.toml)")
                fi
                if [ "$has_hooks_path" -eq 0 ]; then
                    problems+=("$match_path: git config core.hooksPath is not set; per-repo git hooks will not fire")
                    regen_wanted=1
                fi
                if [ "$has_pin" != "true" ]; then
                    fix="Add a launcher pin (a \`mockspace_*\` key such as \`mockspace_branch\`) to \`mockspace.toml\`, since \`cargo mock\` refuses to run without one."
                    [ "$regen_wanted" -eq 1 ] && fix+=" Then run \`homma agent regen --repo $match_name\` for the rest."
                else
                    fix="Run \`homma agent regen --repo $match_name\` to fix."
                fi
            fi
        fi

        # The shared tool configs. The templates directory decides which repos
        # want which config, and a second copy of that decision in bash would
        # start disagreeing with the first the day somebody adds a template,
        # which is the exact thing the directory exists to prevent.
        config_out=$(homma --dir "$WS" repo config check --repo "$match_name" 2>&1)
        config_rc=$?
        # `1` is the check having run and found the repo owing something, which is
        # what the advice below is for. Anything else non-zero is the check not
        # having run at all, and the commonest one is this workspace's own manifest
        # failing to parse. Sending somebody to `init` for that is sending them to a
        # command that reads the same manifest and fails the same way, so the two
        # cases say different things.
        if [ "$config_rc" -eq 1 ]; then
            while IFS= read -r line; do
                [ -n "$line" ] && problems+=("$line")
            done <<<"$config_out"
            fix="Run \`homma repo config init --repo $match_name\` to place what is missing."
        elif [ "$config_rc" -ne 0 ]; then
            problems+=("the shared tool configs could not be checked: $config_out")
            fix="Read \`$WS/homma.toml\`; homma could not run against it, so this says nothing about repo \`$match_name\`."
        fi
    fi
fi

# Nothing wrong on either count, which is nearly every call.
if [ "${{#problems[@]}}" -eq 0 ]; then
    exit 0
fi

# Build a multi-line reason for the deny payload.
reason="repo \`$match_name\` ($match_path) is not ready to commit:"
for p in "${{problems[@]}}"; do
    reason+=$'\n  - '"$p"
done
[ -n "$fix" ] && reason+=$'\n'"$fix"

# Emit a structured deny so Claude Code surfaces it cleanly.
# `jq -Rs .` JSON-encodes stdin as a quoted string; trim the outer
# quotes since our format string already supplies them.
escaped=$(printf '%s' "$reason" | jq -Rs . | sed 's/^"//;s/"$//')
printf '{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}}}\n' \
    "$escaped"
exit 0
"##,
        repo_table = repo_table.trim_end(),
    )
}

#[cfg(test)]
pub(crate) mod tests;
