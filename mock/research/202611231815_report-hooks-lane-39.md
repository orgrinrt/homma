# hooks-lane, roadmap step 39: the agent hooks through one dispatcher

Ticket `merge_the_pretooluse_hook_groups_into_a_few_dispatchers`. Op, 22 November, answered "Consolidate
(Recommended)" when asked "Hooks: 178 PreToolUse groups run on every tool call with no if filter, timeout or
async. Consolidate them?" (options: Consolidate (Recommended); Ticket it for later).

## What landed on this branch

- Rounds 202611231639, 202611231716 and 202611231804, all closed. The design is `mock/DESIGN.md.tmpl`, "The agent
  hooks". The host's contract, by event, is in `research/202611231640_agent-hook-host-contract.md`.
- `homma agent regen` writes the dispatcher `.claude/hooks/_dispatch.sh` and its table `_dispatch.json`. There
  are no more per-hook wrappers.
  - `PreToolUse` and `PostToolUse` get one registration for the hooks the call waits on and one for the `async`
    ones. Each registration's matcher joins the entries' alternatives, and its timeout is the longest entry's
    plus 10 seconds. The hooks run side by side, and their answers are folded the way the host folds them.
  - Every other event gets one registration per entry, which names the entry by its key. The entry runs alone
    and its answer is handed on untouched.
  - `[agent] timeout` and `[agent] filter` are added. A manifest row may carry `timeout` and `async`.
  - Each hook runs under the dispatcher's own timer, as its own process group.
- Review: three passes, the most allowed. Every blocker and should-fix it found is fixed. Three notes are left,
  filed as ticket `the_agent_hook_dispatcher_three_notes_left_from_its_last_rev` (f26b378d96e2).
- Merged in: origin/dev, and `work/hooks-lane-gate` (the readiness-gate fix,
  `the_homma_clone_s_commit_readiness_check_says_the_hooks_key`).
- Tests: the filters cmd::dispatch, aggregate, declared, gates and agent pass, 184 at the last full run plus
  the round 202611231804 additions. `tests/regen_dispatch_smoke.rs` and `tests/regen_writes_smoke.rs` pass.
  `cmd::what::accept_tests` fails apart from this work (ticket 66466ad2f9a7).

## Landing order

1. Land homma `work/hooks-lane-39` into dev.
2. Regenerate the live clause-dev clone with the new homma (`homma agent regen`).
3. Then land clause-dev `work/hooks-lane-39` with land-workspace. Its pre-push refuses the new `[agent]` keys
   until homma has them. That branch carries:
   - the manifest's `[agent]` timeout and filter, and the async rows;
   - the build-guard row moved to the script it runs, with a timeout of 300;
   - the `dedup-*` shims removed;
   - the hookdedup README and test;
   - the agent-regen and agent-surfaces wording.
