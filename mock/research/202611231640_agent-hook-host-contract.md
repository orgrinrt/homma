# What the agent host does with several hooks at one event

Read on 2026-11-23 from https://code.claude.com/docs/en/hooks, the hooks reference of the agent host. Where a
line is quoted it is the page's own text; the rest is the page summarised. The page was read in three windows
and a summarising fetch, and is silent where this file says so.

## Handler fields

- `timeout`: "Seconds before canceling." The default is 600 for a `command` hook, lowered to 30 on
  `UserPromptSubmit`, `PreModelSwitch` and `PostModelSwitch` and to 10 on `MessageDisplay`.
- `async`: "If `true`, runs in the background without blocking." Only on `type: "command"` hooks. Such a hook
  "can't block or control Claude's behavior: response fields like `decision`, `permissionDecision`, and
  `continue` have no effect". "After the background process exits, Claude Code delivers the `additionalContext`
  and `systemMessage` fields from the hook's JSON response to Claude on the next conversation turn." "Once an
  async hook is running in the background, Claude Code doesn't enforce `timeout` on it."
- `if`: one permission rule, such as `Bash(git *)` or `Edit(*.ts)`, evaluated only on `PreToolUse`,
  `PostToolUse`, `PostToolUseFailure`, `PermissionRequest` and `PermissionDenied`; "On other events, a hook with
  `if` set never runs." "There is no `&&`, `||`, or list syntax." A single-segment directory pattern like
  `Edit(src/**)` "matches only the `src` directory in the working directory and the files under it". Silent on
  absolute paths.

## Several hooks at one event

- "When an event fires, its matching hooks run in parallel."
- "If you define the same handler in more than one settings file, it runs once."
- "When multiple PreToolUse hooks return different decisions, precedence is `deny` > `defer` > `ask` > `allow`."
- "A hook that blocks by exiting 2 routes the same way as `"deny"`: Claude sees the stderr message as the denial
  reason."
- When several hooks return `additionalContext` for the same event, Claude receives all of the values.
- Silent on which `updatedInput` wins where several hooks return one.

## Matchers

- `"*"`, `""` or none: every tool.
- Only letters, digits, `_`, `-`, spaces, `,` and `|`: an exact name, or a list of exact names split on `|` or
  `,`.
- Any other character: a JavaScript regular expression, unanchored, tested with `RegExp.prototype.test`, so
  `Edit.*` matches `NotebookEdit` as well.

## By event (second read, the same day)

Exit 2 means something of its own at each event, which is why only the two tool events are folded:

- `PreToolUse`: "A hook that blocks by exiting 2 routes the same way as "deny": Claude sees the stderr message as
  the denial reason."
- `PostToolUse`: "Shows stderr to Claude; the tool already ran." Its JSON reads `decision` `block` with a
  `reason` that "adds the reason next to the tool result", `additionalContext`, `updatedToolOutput`,
  `updatedMCPToolOutput`. The page is silent on whether exit 2 and a `block` are the same, and on how several
  `PostToolUse` decisions combine.
- `PostToolUseFailure`: "Shows stderr to Claude; the tool already failed."
- `PermissionRequest`: "A hook that exits 2 without a decision object leaves the permission flow unchanged, and its
  stderr is discarded."
- `Stop`: "A hook that blocks by exiting 2 routes the same way as reason: Claude receives the stderr message as the
  explanation for why it should continue."
- `SessionStart`: exit 2 "Shows stderr to user only"; "Claude Code adds a SessionStart hook's plain-text stdout to
  Claude's context."
- `UserPromptSubmit`: exit 2 stderr "is treated the same way as reason"; plain stdout is added to context.
- `PreCompact`: "Exit with code 2 to block compaction"; on exit 0 "Claude Code uses its stdout as instructions for
  the compaction summary."
- `Notification` and `SessionEnd` cannot block.
- Another non-zero exit is a non-blocking error and the action goes ahead; "Claude never sees stderr from a hook
  that exits 0."

What a matcher is tested against: the tool name at the tool and permission events; `source` at `SessionStart`;
`trigger` at `Setup`, `PreCompact` and `PostCompact`; `notification_type` at `Notification`; `agent_type` at
`SubagentStart` and `SubagentStop`; `reason` at `SessionEnd`; `error` at `StopFailure`; `load_reason` at
`InstructionsLoaded`; `command_name` at `UserPromptExpansion`; the server at `Elicitation`; `to_model` at the
model switches. `UserPromptSubmit`, `Stop` and several others take no matcher.

Timeouts: 600 seconds for a command hook, lowered to 30 at `UserPromptSubmit`, `PreModelSwitch` and
`PostModelSwitch` and to 10 at `MessageDisplay`; "SessionEnd hooks have a default timeout of 1.5 seconds."

## Output

- Exit 0 with JSON is the structured answer. `PreToolUse` puts its decision under `hookSpecificOutput`:
  `permissionDecision` (`allow`, `deny`, `ask`, `defer`), `permissionDecisionReason`, `updatedInput`,
  `additionalContext`.
- Exit 2 blocks, its stderr the reason.
- Silent, in the windows read, on another non-zero exit for `PreToolUse`; for `PreModelSwitch` "An exit code
  other than 0 or 2 without valid JSON output is a non-blocking error".
