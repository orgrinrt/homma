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

## Output

- Exit 0 with JSON is the structured answer. `PreToolUse` puts its decision under `hookSpecificOutput`:
  `permissionDecision` (`allow`, `deny`, `ask`, `defer`), `permissionDecisionReason`, `updatedInput`,
  `additionalContext`.
- Exit 2 blocks, its stderr the reason.
- Silent, in the windows read, on another non-zero exit for `PreToolUse`; for `PreModelSwitch` "An exit code
  other than 0 or 2 without valid JSON output is a non-blocking error".
