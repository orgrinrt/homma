//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The dispatcher as the bash it is written as.

use super::{DISPATCHER, FOLDED, TABLE};
use crate::cmd::aggregate::{LANDS_IN_SH, MANAGED_MARK, sh_single_quote_escape};

/// The dispatcher. It finds the workspace from its own location, two levels up,
/// so it names no machine, and `filter` is written in relative to it.
///
/// `_dispatch.sh <event> sync|async` runs, side by side, the entries of a
/// folded event ([`FOLDED`]) the call is theirs, and folds their answers into
/// one. `_dispatch.sh <event> one <key>` runs the entry of that
/// [`super::Entry::key`] alone, where the call is its, and hands its answer on
/// exactly as it gave it; a key the table does not have is said on stderr.
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
    let folded = FOLDED.join(" | ");
    format!(
        r##"#!/usr/bin/env bash
# The agent hooks' dispatcher, written {MANAGED_MARK}.
# `{DISPATCHER} <event> sync|async` runs the entries of `{TABLE}` beside it that the call
# is theirs, side by side, and folds their answers into one; `{DISPATCHER} <event> one <key>`
# runs the entry of that key alone and hands its answer on as it gave it. Bash 3.2 and
# later, and `jq` 1.6 or later.

set -u

HERE=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")" && pwd) || exit 0
WS=$(cd -- "$HERE/../.." && pwd) || exit 0
TABLE="$HERE/{TABLE}"
EVENT=${{1:-}}
MODE=${{2:-sync}}
ONE=${{3:-}}
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
# is for. An entry run alone was matched by the host already.
PLAN=$(printf '%s' "$INPUT" | jq -r --arg event "$EVENT" --arg mode "$MODE" --arg one "$ONE" --slurpfile t "$TABLE" '
    def takes($m; $subject):
        if $subject == null or $m == "" or $m == "*" then true
        elif ($m | test("^[A-Za-z0-9_|, -]*$")) then
            any($m | splits("[|,]") | gsub("^ +| +$"; ""); . == $subject)
        else (try ($subject | test($m)) catch true) end;
    (.tool_name // null) as $subject
    | "FILE=\(.tool_input.file_path // .tool_input.path // "" | @sh)",
      "CWD=\(.cwd // .tool_input.cwd // "" | @sh)",
      "CMD=\(.tool_input.command // "" | @sh)",
      ([$t[0].entries[]
        | select(.event == $event)
        | select(if $mode == "one" then .key == $one
            else ((.async // false) == ($mode == "async")) and takes(.matcher // ""; $subject) end)]
        | if $mode == "one" then .[:1] else . end
        | .[]
        | "RUN+=(\(.run | @sh)); RUNNER+=(\(.runner // "" | @sh)); ARGS+=(\(.args // "" | @sh)); TIMEOUT+=(\(.timeout // 60 | tostring | @sh)); ROOTS+=(\((.roots // []) | map(@sh) | join(" ") | @sh))")
') || {{
    echo "homma's hook dispatcher: could not read the payload or $TABLE, so no $EVENT hook ran" >&2
    exit 1
}}

FILE=; CWD=; CMD=
RUN=(); RUNNER=(); ARGS=(); TIMEOUT=(); ROOTS=()
eval "$PLAN"
if [ ${{#RUN[@]}} -eq 0 ]; then
    # A registration of its own names an entry the table no longer has: the
    # settings and the table were written by different regens.
    if [ "$MODE" = one ]; then
        echo "homma's hook dispatcher: no $EVENT entry $ONE in $TABLE, so the settings are not the table's; run \`homma agent regen\`" >&2
        exit 1
    fi
    exit 0
fi
[ -n "$CWD" ] || CWD=$PWD

{LANDS_IN_SH}
# prepare <i>: the command line of entry <i> in `ARGV`; 1 where the call is not
# its or its hook is not in this clone, 2 where its arguments are not words.
prepare() {{
    local r hit
    roots=()
    eval "roots=(${{ROOTS[$1]}})"
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
        [ -n "$hit" ] || return 1
    fi
    hook=${{RUN[$1]}}
    case "$hook" in
        /*) ;;
        *) hook="$WS/$hook" ;;
    esac
    pre=()
    post=()
    # Read as the host's shell would have read them on its command line, an
    # unset variable as nothing.
    set +u
    eval "pre=(${{RUNNER[$1]}})" 2>/dev/null || {{ set -u; return 2; }}
    eval "post=(${{ARGS[$1]}})" 2>/dev/null || {{ set -u; return 2; }}
    set -u
    # Not in this clone: there is no hook here to run.
    if [ ${{#pre[@]}} -gt 0 ]; then
        [ -f "$hook" ] || return 1
    else
        [ -x "$hook" ] || return 1
    fi
    ARGV=(${{pre[@]+"${{pre[@]}}"}} "$hook" ${{post[@]+"${{post[@]}}"}})
}}

if [ "$MODE" = one ]; then
    prepare 0
    case $? in
        1) exit 0 ;;
        2)
            echo "homma's hook dispatcher: ${{RUN[0]}}: its arguments are not words a shell reads" >&2
            exit 1
            ;;
    esac
    exec "${{ARGV[@]}}" <<<"$INPUT"
fi

case "$EVENT" in
    {folded}) ;;
    *)
        echo "homma's hook dispatcher: $EVENT is not an event whose hooks are folded" >&2
        exit 1
        ;;
esac

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t homma-dispatch) || {{
    echo "homma's hook dispatcher: no temporary directory, so no $EVENT hook ran" >&2
    exit 1
}}
trap 'rm -rf "$TMP"' EXIT

# Each hook under its own timer: past its seconds it is sent TERM, and KILL five
# seconds later, and the mark says it was stopped rather than that it failed.
# The hook and its timer each lead a process group of their own, so a signal
# reaches whatever either started, and the timer is gone with its sleep the
# moment the hook is done.
RAN=()
n=0
while [ $n -lt ${{#RUN[@]}} ]; do
    i=$n
    n=$((n + 1))
    prepare "$i"
    case $? in
        1) continue ;;
        2)
            printf 'its arguments are not words a shell reads\n' >"$TMP/$i.err"
            : >"$TMP/$i.out"
            echo 1 >"$TMP/$i.rc"
            RAN=(${{RAN[@]+"${{RAN[@]}}"}} "$i")
            continue
            ;;
    esac
    (
        set -m
        "${{ARGV[@]}}" <<<"$INPUT" >"$TMP/$i.out" 2>"$TMP/$i.err" &
        pid=$!
        (
            sleep "${{TIMEOUT[$i]}}"
            : >"$TMP/$i.late"
            kill -TERM -- -"$pid" 2>/dev/null
            sleep 5
            kill -KILL -- -"$pid" 2>/dev/null
        ) </dev/null >/dev/null 2>&1 &
        timer=$!
        wait "$pid"
        echo $? >"$TMP/$i.rc"
        kill -TERM -- -"$timer" 2>/dev/null
    ) </dev/null >/dev/null 2>&1 &
    RAN=(${{RAN[@]+"${{RAN[@]}}"}} "$i")
done
wait
[ ${{#RAN[@]}} -gt 0 ] || exit 0

# The hooks that said something: a status other than 0, or anything on stdout.
SAID=()
for i in "${{RAN[@]}}"; do
    rc=$(cat "$TMP/$i.rc" 2>/dev/null) || rc=1
    [ -e "$TMP/$i.late" ] && rc=late
    echo "$rc" >"$TMP/$i.rc"
    if [ "$rc" != 0 ] || [ -s "$TMP/$i.out" ]; then
        SAID=(${{SAID[@]+"${{SAID[@]}}"}} "$i")
    fi
done
[ ${{#SAID[@]}} -gt 0 ] || exit 0

# One hook said something: its answer is handed on exactly as it gave it.
if [ ${{#SAID[@]}} -eq 1 ]; then
    i=${{SAID[0]}}
    rc=$(cat "$TMP/$i.rc")
    if [ "$rc" = late ]; then
        echo "${{RUN[$i]}}: ran past its ${{TIMEOUT[$i]}} seconds and was stopped" >&2
        cat "$TMP/$i.err" >&2
        exit 1
    fi
    cat "$TMP/$i.out"
    cat "$TMP/$i.err" >&2
    exit "$rc"
fi

# Several: each answer as the JSON the host would read it as. Exit 2 is a deny
# before the call and a block after it, with its stderr the reason; a failure or
# a stop is a message to the person; JSON that is not one object of the host's
# shape is a failure; plain text, which on a tool event reaches only the
# transcript, goes to stderr.
PARTS=()
BLOCKED=
for i in "${{SAID[@]}}"; do
    rc=$(cat "$TMP/$i.rc")
    case "$rc" in
        0)
            if jq -se 'length == 1 and (.[0] | type == "object") and ((.[0].hookSpecificOutput // {{}}) | type == "object")' "$TMP/$i.out" >/dev/null 2>&1; then
                jq -s '.[0]' "$TMP/$i.out" >"$TMP/$i.json"
            elif jq -s . "$TMP/$i.out" >/dev/null 2>&1; then
                jq -n --arg m "${{RUN[$i]}}: answered JSON the host does not read" '{{systemMessage: $m}}' >"$TMP/$i.json"
            else
                cat "$TMP/$i.out" >&2
                continue
            fi
            ;;
        2)
            BLOCKED=1
            jq -n --arg event "$EVENT" --rawfile why "$TMP/$i.err" '($why | sub("\n+$"; "")) as $w
                | if $event == "PreToolUse"
                  then {{hookSpecificOutput: {{hookEventName: $event, permissionDecision: "deny", permissionDecisionReason: $w}}}}
                  else {{decision: "block", reason: $w}} end' >"$TMP/$i.json"
            ;;
        late)
            jq -n --arg m "${{RUN[$i]}}: ran past its ${{TIMEOUT[$i]}} seconds and was stopped" --rawfile err "$TMP/$i.err" \
                '{{systemMessage: ([$m, ($err | sub("\n+$"; ""))] | map(select(. != "")) | join(": "))}}' >"$TMP/$i.json"
            ;;
        *)
            jq -n --arg m "${{RUN[$i]}}: exited $rc" --rawfile err "$TMP/$i.err" \
                '{{systemMessage: ([$m, ($err | sub("\n+$"; ""))] | map(select(. != "")) | join(": "))}}' >"$TMP/$i.json"
            ;;
    esac
    PARTS=(${{PARTS[@]+"${{PARTS[@]}}"}} "$TMP/$i.json")
done
[ ${{#PARTS[@]}} -gt 0 ] || exit 0

# Folded the way the host folds several hooks: deny over defer over ask over
# allow with the winning decision's reasons, every context and message kept,
# continue false and a block where any says so, any other field the first's.
if ! jq -s --arg event "$EVENT" '
    def joined(f): [.[] | f | select(type == "string" and . != "")]
        | if length == 0 then null else join("\n\n") end;
    def rank: . as $d | {{"deny": 4, "defer": 3, "ask": 2, "allow": 1}} | (.[$d] // 0);
    def clean: with_entries(select(.value != null));
    . as $all
    | [$all[] | (.hookSpecificOutput // {{}})] as $h
    | ([$h[] | .permissionDecision // empty] | max_by(rank)) as $win
    | ($all | joined(.hookSpecificOutput.additionalContext)) as $ctx
    | (reduce $all[] as $o ({{}}; ($o | del(.hookSpecificOutput)) + .))
    | .systemMessage = ($all | joined(.systemMessage))
    | .stopReason = ($all | joined(.stopReason))
    | .continue = (if any($all[]; .continue == false) then false else null end)
    | .suppressOutput = (if any($all[]; .suppressOutput == true) then true else null end)
    | .decision = (if any($all[]; .decision == "block") then "block" else .decision end)
    | .reason = (if .decision == "block"
        then ([$all[] | select(.decision == "block")] | joined(.reason))
        else ($all | joined(.reason)) end)
    | if ([$h[] | select(length > 0)] | length) > 0 then
        .hookSpecificOutput = ((reduce $h[] as $o ({{}}; $o + .))
            + {{hookEventName: $event, additionalContext: $ctx}}
            + (if $win == null then {{}} else {{
                permissionDecision: $win,
                permissionDecisionReason: ([$h[] | select(.permissionDecision == $win)] | joined(.permissionDecisionReason))
              }} end)
            | clean)
      else . end
    | clean
' "${{PARTS[@]}}"; then
    # The fold could not run; a refusal any hook gave still stands, with each
    # refusing hook's reasons.
    if [ -n "$BLOCKED" ] || grep -qE '"(permissionDecision|decision)" *: *"(deny|block)"' "${{PARTS[@]}}" 2>/dev/null; then
        echo "homma's hook dispatcher: the $EVENT answers could not be folded, and one of them refused the call" >&2
        for i in "${{SAID[@]}}"; do
            [ -f "$TMP/$i.json" ] || continue
            grep -qE '"(permissionDecision|decision)" *: *"(deny|block)"' "$TMP/$i.json" 2>/dev/null || continue
            why=$(jq -r '[.hookSpecificOutput.permissionDecisionReason?, .reason?] | map(select(type == "string" and . != "")) | join("\n")' "$TMP/$i.json" 2>/dev/null) \
                || why=$(cat "$TMP/$i.json")
            printf '%s: %s\n' "${{RUN[$i]}}" "$why" >&2
        done
        exit 2
    fi
    echo "homma's hook dispatcher: the $EVENT answers could not be folded" >&2
    exit 1
fi
exit 0
"##
    )
}
