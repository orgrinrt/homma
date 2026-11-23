# src changelist

Topic: `202611230833-tgxe_topic.release-review-fixes.md`; the doc changelist is locked.

## mock/crates/homma-core

- `src/release/auth/store.rs`: `Store` gains `writable(key)`, defaulting to `Ok`, which says before any paste
  whether a write could land; `Environment` answers with its named refusal. `Stored`'s `Debug` is written by
  hand, the secret printed as `<redacted>`.
- `src/release/auth/mod.rs`: `State::Refused`, which needs work, marks `REFUSED` and displays `refused`.
- `src/release/auth/flow.rs`: `fix_token` asks `writable` first, saying the refusal and returning
  `State::Refused` before any paste; a `put` refused after a proven token returns `Refused` too, where it was
  `Unreachable`. `all`'s closing line no longer promises a token will fix what it names.
- `src/release/auth/tests.rs` is replaced by `src/release/auth/tests/`: `mod.rs` (the shared doubles), `grade.rs`
  (expiry and each registry's answer), `fix.rs` (the paste, prove, store order, the refusing store, the exit
  of `all`), `npmrc.rs`, `store.rs` (the keychain program that is not there, the environment store, the comment
  date, the `Debug` rendering), `verbs.rs` (env, token, expiry, diagnose) and `live.rs` (the request, over a local
  port), the macOS keychain round trip in `store.rs`.
- `src/release/packages.rs`: `tag_name_or(tags, version, fresh)`, the repo's convention where it has tags and
  `fresh` where it has none; `tag_name` is `tag_name_or` with `v`, so `release run` is unchanged.
  `src/release/check_tests.rs` gains the fresh-repo cases.
- `src/release/hygiene_tests.rs`: a fixture with a bare remote and a `dev` branch, and for each of the twelve rows
  that no test fired (`tree.uptodate`, `tree.worktrees`, `branch.trunk`, `branch.merged`, `branch.unmerged`,
  `branch.orphan`, `ref.filterrepo`, `tag.order.matches`, `hist.workflow.past`, `hist.large`, `semver.parses`,
  `semver.gaps.closable`) a test that fires it and one that keeps it quiet; the clean-repo test also runs on the
  remote fixture.
- `src/release/history_tests.rs`: none. `releasable(["0.1.0", "0.0.0"]) == []` is already asserted, in
  `nothing_survives_a_line_that_is_only_scaffold`.
- `src/tls_tests.rs`: none. Two full runs, one of them traced for every directory made or removed, found no test
  that touches `src/`: each one that makes a directory does it in a temporary one, and the run was green. The ENOENT
  did not reproduce and nothing is changed for it; the reason is reported.

## mock/crates/homma-engine

- `src/cmd/release/versions.rs`: `tag` names with `tag_name_or(.., "")`, bare where the repo has no tag.
- `src/cmd/release/estate.rs`: `--json` beside `jumps` or `missing` is refused with a reason, nothing written.
- `tests/cli_release.rs`: `auth fix` on a machine whose store refuses writes exits non-zero naming the variable
  and asks for no paste; `estate jumps --json` is refused.

## Other flat topics

None beyond this round's one.
