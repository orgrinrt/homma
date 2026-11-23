//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The one dispatcher every agent hook runs through, and the table it reads.
//!
//! The regen gathers an [`Entry`] for every hook a member gives, every row the
//! manifest declares and the workspace gate, writes them to
//! `.claude/hooks/_dispatch.json`, writes the dispatcher beside it, and returns
//! one registration per event and mode for [`super::aggregate::merge_settings`]:
//! the hooks the call waits on, and the `async` ones. The dispatcher is a shell
//! script, so a tool call never waits on homma's engine being built for a new
//! pin; it reads the payload and the table with one `jq`, runs the entries the
//! call is theirs side by side, each under its timeout, and folds their answers
//! into the one the host reads.

use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result, anyhow};
use homma_api::Root;
use serde::{Deserialize, Serialize};

use crate::cmd::aggregate::{
    HookEntry,
    LANDS_IN_SH,
    MANAGED_MARK,
    carries_the_mark,
    sh_single_quote_escape,
};

/// The dispatcher's file name under `.claude/hooks/`.
pub(crate) const DISPATCHER: &str = "_dispatch.sh";

/// The table's file name under `.claude/hooks/`.
pub(crate) const TABLE: &str = "_dispatch.json";

/// The `from` of an entry made from a row of the manifest.
pub(crate) const FROM_ROWS: &str = "_declared";

/// The `from` of the workspace gate's entry.
pub(crate) const FROM_GATE: &str = "_workspace";

/// The seconds the host gives the dispatcher over its longest entry, so the
/// host never cuts short a hook the dispatcher would have let run.
const SLACK: u32 = 10;

/// One hook the dispatcher runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// The host event it runs at.
    pub event:      String,
    /// The tools it fires for, as the host reads a matcher; empty or `*` for
    /// every one.
    pub matcher:    String,
    /// The file it runs, relative to the workspace, or absolute for a member
    /// declared outside it.
    pub run:        String,
    /// The program it is run with, as written; empty where the file runs itself.
    pub runner:     String,
    /// Its arguments, as written.
    pub args:       String,
    /// The repositories it is narrowed to, relative to the workspace; empty for
    /// every call.
    pub roots:      Vec<String>,
    /// The seconds it may run.
    pub timeout:    u32,
    /// Whether it runs with the hooks nothing waits on.
    #[serde(rename = "async")]
    pub background: bool,
    /// The member it came from, [`FROM_ROWS`] or [`FROM_GATE`].
    pub from:       String,
}

/// The table as it is written: the line that makes it homma's, and the entries.
#[derive(Debug, Serialize, Deserialize)]
struct Table {
    written: String,
    entries: Vec<Entry>,
}

/// The entries the last regen wrote, or none where there is no table, or one
/// homma did not write, or one it cannot read.
pub(crate) fn previous(root: &Root) -> Vec<Entry> {
    let path = root.as_abs().join(".claude/hooks").join(TABLE);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Table>(&s).ok())
        .filter(|t| t.written.contains(MANAGED_MARK))
        .map(|t| t.entries)
        .unwrap_or_default()
}

/// Whether the table at `path` is absent or homma's.
fn table_is_ours(path: &std::path::Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Table>(&s).ok())
        .is_some_and(|t| t.written.contains(MANAGED_MARK))
}

/// Whether a registration runs the dispatcher.
pub(crate) fn is_dispatcher_command(cmd: &str) -> bool {
    crate::cmd::aggregate::hook_file_named(cmd).is_some_and(|n| n == DISPATCHER)
}

/// Write the table and the dispatcher, remove every wrapper an earlier regen
/// wrote, and return the dispatcher's registrations.
///
/// `filter` is `[agent] filter`, the program the dispatcher runs itself
/// through. A table or a dispatcher homma did not write is refused by path
/// rather than written over.
pub(crate) fn install(root: &Root, entries: &[Entry], filter: &[String]) -> Result<Vec<HookEntry>> {
    let hooks_dir = root
        .contain(&root.as_abs().join(".claude/hooks"))
        .map_err(|e| anyhow!("{e}"))?;
    root.create_dir_all(&hooks_dir).ok();
    let script = root
        .contain_under(&hooks_dir, DISPATCHER)
        .map_err(|e| anyhow!("{e}"))?;
    let table = root
        .contain_under(&hooks_dir, TABLE)
        .map_err(|e| anyhow!("{e}"))?;
    if script.as_path().exists() && !carries_the_mark(script.as_path()) {
        return Err(anyhow!(
            "`.claude/hooks/{DISPATCHER}` was not written by homma, so the dispatcher is not \
             written over it; move that file aside or rename it"
        ));
    }
    if !table_is_ours(table.as_path()) {
        return Err(anyhow!(
            "`.claude/hooks/{TABLE}` was not written by homma, so the dispatcher's table is not \
             written over it; move that file aside or rename it"
        ));
    }

    let body = serde_json::to_string_pretty(&Table {
        written: format!("this file is written {MANAGED_MARK}"),
        entries: entries.to_vec(),
    })?;
    root.write(&table, body + "\n")
        .with_context(|| format!("write {}", table.as_path().display()))?;
    root.write(&script, dispatcher_script(filter))
        .with_context(|| format!("write {}", script.as_path().display()))?;
    #[cfg(unix)]
    root.set_executable(&script)?;

    retire_wrappers(root, &hooks_dir)?;
    Ok(registrations(entries))
}

/// Remove every file under `.claude/hooks/` an earlier regen wrote as a wrapper:
/// whatever carries the mark but the dispatcher and the workspace gate.
fn retire_wrappers(root: &Root, hooks_dir: &homma_api::ContainedPath) -> Result<()> {
    for entry in fs::read_dir(hooks_dir.as_path())
        .with_context(|| format!("read {}", hooks_dir.as_path().display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let Some(n) = name.to_str() else {
            continue;
        };
        if n == DISPATCHER || n == crate::cmd::gates::GATE_SCRIPT_NAME {
            continue;
        }
        let target = root
            .contain_under(hooks_dir, &name)
            .map_err(|e| anyhow!("{e}"))?;
        if target.as_path().is_file() && carries_the_mark(target.as_path()) {
            root.remove_file(&target).ok();
        }
    }
    Ok(())
}

/// One registration per event and mode that has entries: the matchers joined,
/// or every tool where one entry takes every tool, and the longest timeout with
/// [`SLACK`] over.
pub(crate) fn registrations(entries: &[Entry]) -> Vec<HookEntry> {
    let mut groups: BTreeMap<(&str, bool), Vec<&Entry>> = BTreeMap::new();
    for e in entries {
        groups
            .entry((e.event.as_str(), e.background))
            .or_default()
            .push(e);
    }
    groups
        .into_iter()
        .map(|((event, background), es)| {
            let none = |e: &&Entry| e.matcher.trim().is_empty();
            let every = es.iter().any(|e| none(e) || e.matcher.trim() == "*");
            // None at all where no entry named one, as their registrations
            // were written; every tool where any entry takes every tool.
            let matcher = if es.iter().all(none) {
                String::new()
            } else if every {
                "*".to_string()
            } else {
                let mut seen: Vec<&str> = Vec::new();
                for e in &es {
                    if !seen.contains(&e.matcher.as_str()) {
                        seen.push(e.matcher.as_str());
                    }
                }
                seen.join("|")
            };
            let longest = es.iter().map(|e| e.timeout).max().unwrap_or(0);
            let mode = if background { " async" } else { "" };
            HookEntry {
                event: event.to_string(),
                matcher,
                command: format!(
                    "\"${{CLAUDE_PROJECT_DIR}}\"/.claude/hooks/{DISPATCHER} {event}{mode}"
                ),
                timeout: Some(longest + SLACK),
                background,
            }
        })
        .collect()
}

/// The dispatcher. It finds the workspace from its own location, two levels up,
/// so it names no machine, and `filter` is written in relative to it.
pub(crate) fn dispatcher_script(filter: &[String]) -> String {
    let filter = match filter.split_first() {
        None => String::new(),
        Some((program, rest)) => {
            let mut words = vec![format!("\"$WS\"/'{}'", sh_single_quote_escape(program))];
            words.extend(
                rest.iter()
                    .map(|w| format!("'{}'", sh_single_quote_escape(w))),
            );
            words.join(" ")
        },
    };
    format!(
        r##"#!/usr/bin/env bash
# The agent hooks' dispatcher, written {MANAGED_MARK}.
# `_dispatch.sh <event> [async]` runs the entries of `_dispatch.json` beside it
# that the call is theirs, side by side, and folds their answers into one. Bash
# 3.2 and later, and `jq`.

set -u

HERE=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")" && pwd) || exit 0
WS=$(cd -- "$HERE/../.." && pwd) || exit 0
TABLE="$HERE/{TABLE}"
EVENT=${{1:-}}
MODE=${{2:-sync}}
[ -n "$EVENT" ] || {{ echo "homma's hook dispatcher: no event named" >&2; exit 1; }}

# Once round the whole dispatch, where the workspace names a filter and has it.
FILTER=({filter})
if [ -z "${{HOMMA_DISPATCH_FILTERED:-}}" ] && [ ${{#FILTER[@]}} -gt 0 ] && [ -x "${{FILTER[0]}}" ]; then
    HOMMA_DISPATCH_FILTERED=1 exec "${{FILTER[@]}}" "${{BASH_SOURCE[0]}}" "$@"
fi

[ -f "$TABLE" ] || exit 0
if ! command -v jq >/dev/null 2>&1; then
    echo "homma's hook dispatcher: jq is not on PATH, so it cannot read its table and no $EVENT hook ran" >&2
    exit 1
fi

INPUT=$(cat)

# One read of the payload and the table: where the call lands, then each entry
# this handler runs, as shell words. A matcher is read the way the host reads
# one, and one jq cannot read takes the call, since a guard skipped costs what it
# is for.
PLAN=$(printf '%s' "$INPUT" | jq -r --arg event "$EVENT" --arg mode "$MODE" --slurpfile t "$TABLE" '
    def takes($m; $subject):
        if $subject == null or $m == "" or $m == "*" then true
        elif ($m | test("^[A-Za-z0-9_|, -]*$")) then
            any($m | splits("[|,]") | gsub("^ +| +$"; ""); . == $subject)
        else (try ($subject | test($m)) catch true) end;
    (.tool_name // .source // .trigger // null) as $subject
    | "FILE=\(.tool_input.file_path // .tool_input.path // "" | @sh)",
      "CWD=\(.cwd // .tool_input.cwd // "" | @sh)",
      "CMD=\(.tool_input.command // "" | @sh)",
      ($t[0].entries[]
        | select(.event == $event)
        | select((.async // false) == ($mode == "async"))
        | select(takes(.matcher // ""; $subject))
        | "RUN+=(\(.run | @sh)); RUNNER+=(\(.runner // "" | @sh)); ARGS+=(\(.args // "" | @sh)); TIMEOUT+=(\(.timeout // 60 | tostring | @sh)); ROOTS+=(\((.roots // []) | map(@sh) | join(" ") | @sh))")
') || {{
    echo "homma's hook dispatcher: could not read the payload or $TABLE, so no $EVENT hook ran" >&2
    exit 1
}}

FILE=; CWD=; CMD=
RUN=(); RUNNER=(); ARGS=(); TIMEOUT=(); ROOTS=()
eval "$PLAN"
[ ${{#RUN[@]}} -gt 0 ] || exit 0
[ -n "$CWD" ] || CWD=$PWD

{LANDS_IN_SH}
TMP=$(mktemp -d 2>/dev/null || mktemp -d -t homma-dispatch) || {{
    echo "homma's hook dispatcher: no temporary directory, so no $EVENT hook ran" >&2
    exit 1
}}
trap 'rm -rf "$TMP"' EXIT
LIMIT=
command -v timeout >/dev/null 2>&1 && LIMIT=timeout

RAN=()
n=0
while [ $n -lt ${{#RUN[@]}} ]; do
    i=$n
    n=$((n + 1))
    roots=()
    eval "roots=(${{ROOTS[i]}})"
    if [ ${{#roots[@]}} -gt 0 ]; then
        hit=
        for r in "${{roots[@]}}"; do
            case "$r" in
                /*) ;;
                *) r="$WS/$r" ;;
            esac
            if lands_in "$r"; then
                hit=1
                break
            fi
        done
        [ -n "$hit" ] || continue
    fi
    hook=${{RUN[i]}}
    case "$hook" in
        /*) ;;
        *) hook="$WS/$hook" ;;
    esac
    pre=()
    post=()
    eval "pre=(${{RUNNER[i]}})"
    eval "post=(${{ARGS[i]}})"
    # Not in this clone: there is no hook here to run.
    if [ ${{#pre[@]}} -gt 0 ]; then
        [ -f "$hook" ] || continue
    else
        [ -x "$hook" ] || continue
    fi
    cmd=()
    [ -n "$LIMIT" ] && cmd=("$LIMIT" -k 5 "${{TIMEOUT[i]}}")
    [ ${{#pre[@]}} -gt 0 ] && cmd=(${{cmd[@]+"${{cmd[@]}}"}} "${{pre[@]}}")
    cmd=(${{cmd[@]+"${{cmd[@]}}"}} "$hook")
    [ ${{#post[@]}} -gt 0 ] && cmd=("${{cmd[@]}}" "${{post[@]}}")
    (
        "${{cmd[@]}}" <<<"$INPUT" >"$TMP/$i.out" 2>"$TMP/$i.err"
        echo $? >"$TMP/$i.rc"
    ) &
    RAN=(${{RAN[@]+"${{RAN[@]}}"}} "$i")
done
wait
[ ${{#RAN[@]}} -gt 0 ] || exit 0

# Fold the answers: a block wins, JSON folds the way the host folds several
# hooks, plain text stays text, and any other failure is named.
blocked=
blocks=
texts=
fails=
JSON=()
for i in "${{RAN[@]}}"; do
    rc=$(cat "$TMP/$i.rc" 2>/dev/null) || rc=1
    case "$rc" in
        0)
            if [ -s "$TMP/$i.out" ]; then
                if jq -e 'type == "object"' "$TMP/$i.out" >/dev/null 2>&1; then
                    JSON=(${{JSON[@]+"${{JSON[@]}}"}} "$TMP/$i.out")
                else
                    texts="$texts$(cat "$TMP/$i.out")"$'\n'
                fi
            fi
            ;;
        2)
            blocked=1
            [ -s "$TMP/$i.err" ] && blocks="$blocks$(cat "$TMP/$i.err")"$'\n'
            ;;
        124 | 137)
            fails="$fails${{RUN[i]}}: ran past its ${{TIMEOUT[i]}} seconds and was stopped"$'\n'
            [ -s "$TMP/$i.err" ] && fails="$fails$(cat "$TMP/$i.err")"$'\n'
            ;;
        *)
            fails="$fails${{RUN[i]}}: exited $rc"$'\n'
            [ -s "$TMP/$i.err" ] && fails="$fails$(cat "$TMP/$i.err")"$'\n'
            ;;
    esac
done

if [ -n "$blocked" ]; then
    printf '%s' "$blocks" >&2
    if [ ${{#JSON[@]}} -gt 0 ]; then
        ctx=$(jq -rs '[.[] | .hookSpecificOutput.additionalContext? // empty | select(type == "string" and . != "")] | join("\n\n")' "${{JSON[@]}}" 2>/dev/null)
        [ -n "$ctx" ] && printf '\n%s\n' "$ctx" >&2
    fi
    [ -n "$fails" ] && printf '%s' "$fails" >&2
    exit 2
fi

[ -n "$fails" ] && printf '%s' "$fails" >&2
if [ ${{#JSON[@]}} -eq 0 ]; then
    if [ -n "$texts" ]; then
        printf '%s' "$texts"
        exit 0
    fi
    [ -z "$fails" ] || exit 1
    exit 0
fi

case "$EVENT" in
    SessionStart | UserPromptSubmit) ;;
    *)
        [ -n "$texts" ] && printf '%s' "$texts" >&2
        texts=
        ;;
esac
jq -s --arg event "$EVENT" --arg text "$texts" '
    def joined(f): [.[] | f | select(type == "string" and . != "")]
        | if length == 0 then null else join("\n\n") end;
    def rank: . as $d | {{"deny": 4, "defer": 3, "ask": 2, "allow": 1}} | (.[$d] // 0);
    def clean: with_entries(select(.value != null));
    . as $all
    | [$all[] | (.hookSpecificOutput // {{}})] as $h
    | ([$h[] | .permissionDecision // empty] | max_by(rank)) as $win
    | ([$all | joined(.hookSpecificOutput.additionalContext), ($text | sub("\n+$"; ""))]
        | map(select(. != null and . != "")) | if length == 0 then null else join("\n\n") end) as $ctx
    | (reduce $all[] as $o ({{}}; ($o | del(.hookSpecificOutput)) + .))
    | .systemMessage = ($all | joined(.systemMessage))
    | .stopReason = ($all | joined(.stopReason))
    | .continue = (if any($all[]; .continue == false) then false else null end)
    | .suppressOutput = (if any($all[]; .suppressOutput == true) then true else null end)
    | .decision = (if any($all[]; .decision == "block") then "block" else .decision end)
    | .reason = (if .decision == "block"
        then ([$all[] | select(.decision == "block")] | joined(.reason))
        else ($all | joined(.reason)) end)
    | if ([$h[] | select(length > 0)] | length) > 0 or $ctx != null then
        .hookSpecificOutput = ((reduce $h[] as $o ({{}}; $o + .))
            + {{hookEventName: $event, additionalContext: $ctx}}
            + (if $win == null then {{}} else {{
                permissionDecision: $win,
                permissionDecisionReason: ([$h[] | select(.permissionDecision == $win)] | joined(.permissionDecisionReason))
              }} end)
            | clean)
      else . end
    | clean
' "${{JSON[@]}}"
"##
    )
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
pub(in crate::cmd) mod tests;

#[cfg(test)]
#[path = "dispatch_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "dispatch_fold_tests.rs"]
mod fold_tests;
