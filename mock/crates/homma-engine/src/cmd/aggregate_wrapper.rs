//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a registration names its hook, and the wrapper homma writes in front of
//! one: the scope check that decides whether a call lands in the hook's
//! repository, shared by every wrapper homma writes.

/// What a registration's command runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookCall {
    /// The hook file, by its name under `.claude/hooks/`.
    pub name:   String,
    /// The words before the hook's path, which the host runs it with. Empty
    /// where the host runs the file itself.
    pub runner: String,
    /// Whatever followed the hook's path, as it was written.
    pub args:   String,
}

/// The hook a command runs: the first word reaching into `.claude/hooks/`,
/// whatever prefix it reaches it through, a relative path, the host's
/// project-root placeholder or an absolute one. None for a command running
/// nothing there, or something in a directory below it.
pub(crate) fn hook_call(cmd: &str) -> Option<HookCall> {
    let mut rest = cmd.trim_start();
    let mut before: Vec<&str> = Vec::new();
    while !rest.is_empty() {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        let after = after.trim_start();
        if word.contains(".claude/hooks/") {
            let bare: String = word.chars().filter(|c| *c != '"' && *c != '\'').collect();
            let (_, name) = bare.rsplit_once(".claude/hooks/")?;
            if name.is_empty() || name.contains('/') {
                return None;
            }
            return Some(HookCall {
                name:   name.to_string(),
                runner: before.join(" "),
                args:   after.trim_end().to_string(),
            });
        }
        before.push(word);
        rest = after;
    }
    None
}

/// The one program every registration of a hook runs it with, or the problem
/// to report for `shown` where there is not one.
///
/// Two registrations running one hook two ways cannot share a wrapper, since it
/// runs the hook one way. A runner that is a line of shell rather than a
/// program is refused too: the wrapper puts it in front of the hook in an
/// `exec`, which runs a program with arguments, so an assignment in front of
/// the hook fails there and a separator before it runs something else. `cd dir
/// && .claude/hooks/x.sh` would `exec` a `cd` binary, which exits 0, and that
/// would read as the hook's approval.
pub(crate) fn one_runner<'a>(
    runners: impl Iterator<Item = &'a str>,
    shown: &str,
) -> Result<String, String> {
    let runners: std::collections::BTreeSet<&str> = runners.collect();
    if runners.len() > 1 {
        let ways: Vec<String> = runners
            .iter()
            .map(|r| if r.is_empty() { "itself".to_string() } else { format!("`{r}`") })
            .collect();
        return Err(format!(
            "`{shown}` is run through {} by different registrations, and one wrapper can \
             only run it one way; not carried",
            ways.join(" and ")
        ));
    }
    let runner = runners.first().copied().unwrap_or("");
    let assignment = runner
        .split_whitespace()
        .next()
        .is_some_and(|w| w.contains('='));
    let shell = ['&', ';', '|', '(', ')', '`', '<', '>'];
    if assignment || runner.contains(shell) || runner.contains("$(") {
        return Err(format!(
            "`{shown}` is run through `{runner}`, which is a line of shell rather than a \
             program, and the wrapper can only run a program with arguments; not carried"
        ));
    }
    Ok(runner.to_string())
}

/// The file a hook command runs, by its name under `.claude/hooks/`.
pub(crate) fn hook_file_named(cmd: &str) -> Option<String> {
    hook_call(cmd).map(|c| c.name)
}

/// Escape a string for embedding inside a bash single-quoted literal.
/// Replaces each `'` with `'\''` so the generated script is safe for
/// paths or names that happen to contain a single quote.
pub(crate) fn sh_single_quote_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// The scope check every wrapper carries, as bash reading the host's JSON from
/// `$INPUT`. It defines `lands_in <dir>`, true when the call lands anywhere
/// under that directory: a file tool on its file, a shell call in its directory
/// and on every path its command names, read relative to that directory. Where
/// the host names no directory, the wrapper's own stands in, which is the
/// workspace root whatever the session did.
///
/// A command is split on everything that cannot be part of a path, which takes
/// quotes, separators, redirections and the `=` of `--flag=path` apart. A file
/// or a piece climbing with `..` lands everywhere, and a directory's absolute path anywhere
/// in the command counts, which is what catches one with a space in it. Not
/// seen: a relative path with a space in it, a path spelled through a variable,
/// and what a script the command runs does next.
pub(crate) const LANDS_IN_SH: &str = r#"FILE=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // empty' 2>/dev/null)
CWD=$(printf '%s' "$INPUT" | jq -r '.cwd // .tool_input.cwd // empty' 2>/dev/null)
CMD=$(printf '%s' "$INPUT" | jq -r '.tool_input.command // empty' 2>/dev/null)
[ -n "$CWD" ] || CWD=$PWD

under() {
    case "$2" in
        "$1"|"$1"/*) return 0 ;;
    esac
    return 1
}

# A path or piece climbing with `..` lands everywhere, since a guard skipped
# costs what the guard is for. Running one too often is not free either: a
# guard that denies can deny a call that never touched its repository.
lands_in() {
    local root=$1 w
    if [ -n "$FILE" ]; then
        case "$FILE" in
            ..|../*|*/..|*/../*) return 0 ;;
        esac
        case "$FILE" in
            /*) under "$root" "$FILE" ;;
            *)  under "$root" "$CWD/$FILE" ;;
        esac
        return
    fi
    under "$root" "$CWD" && return 0
    [ -n "$CMD" ] || return 1
    # The directory's own absolute path, ended by anything a name cannot carry
    # on with, so a neighbour whose name only starts the same is not it.
    case "$CMD" in
        *"$root"|*"$root"[!A-Za-z0-9_.@%+,:~-]*) return 0 ;;
    esac
    local IFS=$' \t\n'
    set -f
    for w in $(printf '%s' "$CMD" | tr -c 'A-Za-z0-9_./~@%+,:-' ' '); do
        case "$w" in
            '~/'*) w=${HOME:-}/${w#'~/'} ;;
            /*) ;;
            *) w=$CWD/$w ;;
        esac
        # A `.` component names the directory it sits in, so `./engine` is
        # `engine`.
        while [ "${w//\/.\//\/}" != "$w" ]; do
            w=${w//\/.\//\/}
        done
        w=${w%/.}
        case "$w" in
            */..|*/../*) set +f; return 0 ;;
        esac
        if under "$root" "$w"; then
            set +f
            return 0
        fi
    done
    set +f
    return 1
}
"#;

/// Build the wrapper script body for an aggregated hook.
///
/// Both paths are relative, and that is the whole of this function.
/// `repo_rel_path` is the repo's path under the workspace, which the manifest
/// already holds for the detected member; `hook_rel_path` is the hook's
/// path under the repo. Neither names a machine. `runner` is the program the
/// registrations run the hook with, as they wrote it, and empty where the host
/// runs the file itself.
///
/// A wrapper sits at `<workspace>/.claude/hooks/<file>`, a fixed depth, so it
/// finds the workspace from its own location and needs no baked prefix and no
/// environment variable. That is what lets a tracked wrapper work in every
/// clone rather than only in the one that generated it.
///
/// The emitted wrapper:
/// 1. Locates the workspace from its own path and derives the repo root and the
///    real hook under it.
/// 2. Exits 0 when there is no hook there to run: not executable, or with a
///    runner, not a file. That is the case where this workspace has not cloned
///    the repo, and it declines rather than reporting an approval it did not
///    make.
/// 3. Hands off without narrowing when `jq` is missing, since it cannot read
///    where the call lands.
/// 4. Exits 0 when the call does not land under the repo root, by
///    [`LANDS_IN_SH`].
/// 5. Otherwise replaces itself with the real hook, passing on its own
///    arguments and re-feeding the original stdin, in the directory the host
///    ran it in. A relative path in the payload is relative to that directory,
///    so running the hook anywhere else would have it misread one.
pub(crate) fn wrapper_script(
    repo_name: &str,
    repo_rel_path: &str,
    hook_rel_path: &str,
    runner: &str,
) -> String {
    let repo_rel = sh_single_quote_escape(repo_rel_path);
    let hook_rel = sh_single_quote_escape(hook_rel_path);
    let present = if runner.is_empty() { "-x" } else { "-f" };
    let run = if runner.is_empty() { String::new() } else { format!("{runner} ") };
    let lands_in = LANDS_IN_SH;
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
[ {present} "$ORIG_HOOK" ] || exit 0

INPUT=$(cat)

# Without `jq` this cannot read where the call lands, so it cannot narrow to
# this repo. It forwards instead of guessing: forwarding is what happens with no
# aggregation at all, so the guard runs more often than it needs to and never
# silently fails to run.
if ! command -v jq >/dev/null 2>&1; then
    exec {run}"$ORIG_HOOK" "$@" <<<"$INPUT"
fi

{lands_in}
lands_in "$REPO_ROOT" || exit 0

exec {run}"$ORIG_HOOK" "$@" <<<"$INPUT"
"##
    )
}

#[cfg(test)]
#[path = "aggregate_wrapper_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "aggregate_wrapper_handoff_tests.rs"]
mod handoff_tests;
