# Review round 2: `work/release-through-forge-route` at `3396a5c`

## Verdict

**MERGE AFTER FIXES.** Round 1's six findings are fixed in the code, and each claimed red goes red under its mutation. Two things block the merge. First, the generated documents are stale; the public-surface rule says that alone blocks. Second, a test-gate gap: the engine-side code that picks route or client per repo has no test.

Canon gate: homma declares no `canon_paths`, so `DEEPDIVE_release.md.tmpl` steps 6 and 7 and the round 202611151306 topic stand in for the canon. The branch matches them in intent. This is my own read, made without relying on round 1's conclusions; a second independent read is still owed.

## Findings

1. `docs/DESIGN.md:114` and `docs/CORE_RELEASE.md:371-390` [public-surface]: the docs were rendered in round 1 (`05a48a2`) and not again after `9db56a9`.
   - `docs/DESIGN.md:114` still carries the unwrapped round 1 sentence ("the tag and the GitHub release are asked of that command"), which the template no longer has.
   - `docs/CORE_RELEASE.md` has no `ReleaseForge` paragraph, no "only for GitHub" condition, and no fetch-and-refuse in step 6.
   - **Fix:** run `cargo mock`, commit the rendered `docs/`, and check that both files match their templates.

2. `mock/crates/homma-engine/src/cmd/release/mod.rs:119` [tests]: no test exercises the per-repo choice itself. `route_tests` only tests `route_for`. If `release_forge` matched on `route` directly instead of `route_for(kind, route)`, a Forgejo repo would go to GitHub again (round 1 finding 3), and every test would stay green. `release_forge` has one caller (`mod.rs:354`) and no test.
   - **Fix:** add a test that calls `release_forge(ForgeKind::Forgejo, Some(route), &recording_runner, ByClient { forge: &recorder, .. })`, then `.release(..)`. Assert the recorder made the release and the runner was never called. Add the GitHub case the other way round.

3. `mock/crates/homma-engine/src/cmd/release/mod.rs:277` [correctness]: `forge_route_of` resolves the path but never checks that a file is there. A manifest whose `forge_route` names a missing file fails at step 6, as a `Spawn` error turned into `ReleaseError::Route`, after `main` was pushed. That is the same "after `main` has moved" failure the topic gives as the reason a bad manifest must be an error.
   - **Fix:** make a named route that does not exist an error at `forge_route_of`, before any repo runs. Since this adds behaviour, add a line for it to the round's doc changelist, plus a red test next to `a_manifest_that_does_not_read_as_a_workspaces_is_an_error_and_not_no_route`.

4. `mock/crates/homma-core/src/release/git.rs:432-433` [naming]: the new doc comment was put under `fetch`'s doc comment rather than above it, so the two run together:
   - `fetch_tag`'s rustdoc now opens with "Fetch tags and branches from `remote`, pruning what is gone", which describes `fetch`, not `fetch_tag`.
   - `fetch` (`:439`) has lost its doc comment.
   - **Fix:** move line 432 back above `pub fn fetch`.

5. `mock/design_rounds/202611151308_changelist.src.md:21-23` and `202611151306_changelist.doc.lock.md:10` [drift]: these say the three tickets are filed in "kaski's registry on `dev`" with ids `c741430640bc`, `41d072890a68` and `686c009ebe68`.
   - None of those ids exists in kaski, and kaski `dev` has no such tickets (fetched, head `2702ce3ec`).
   - The tickets actually exist on clause-dev `origin/main`, in `.shared/scripts/life/tickets-homma.toml` and `tickets-workspace.toml`, under ids that are not the ones cited.
   - **Fix:** correct the citation in the open src changelist. The doc lock is locked, so its wrong line stays unless the round's process allows an addendum.

6. `mock/crates/homma-core/src/release/run_tests.rs:706-720` [tests]: `a_route_that_answers_done_and_made_no_tag_stops_the_release` only asserts `ReleaseError::Git(_)`. Any git failure anywhere in the run (the bump push, the merge) would satisfy it.
   - **Fix:** assert that the route was asked for the tag, that it was never asked for the release, and that the tree is back on `dev`, as the neighbouring tests do.

7. `mock/crates/homma-core/src/release/run.rs:279-290` [correctness, low]: `step_hold_tag` fetches the remote's tag into `refs/tags/<tag>` before comparing commits. On `TagElsewhere`, the clone keeps a version tag on the wrong commit, and the next plan in that clone treats it as the last release.
   - **Fix:** fetch without a destination ref (`git fetch <remote> refs/tags/<tag>`, then compare `FETCH_HEAD^{commit}`) and write the local tag only on a match.
   - This path can actually happen: `forge-route.sh:138` treats a Worker's `409 exists` as done without checking which commit the tag names when `ls-remote` returned nothing.

8. `mock/crates/homma-core/DEEPDIVE_release.md.tmpl:384` [naming]: the `ReleaseForge` paragraph sits between step 7 and step 8 and splits the numbered run in two. The doc changelist says "after the steps". **Fix:** move it below step 8 and the rest of the list.

## Verified

- **Suite:** `cargo test -p homma-api -p homma-core -p homma-engine --no-fail-fast` with the scratch TMPDIR. The only reds are the five `what::accept_tests` and `regen_writes_smoke::a_write_leaving_the_workspace_is_refused_before_the_deny_list_is_consulted`, all known and independent of the branch.
- **Mutations**, run in an archive copy of HEAD:
  - `step_hold_tag` not called: 4 routed tests fail.
  - The commit check removed: `a_tag_the_remote_holds_on_another_commit_refuses_the_release` fails.
  - `route_for` returning the route for Forgejo: `the_route_is_taken_for_a_github_repo_alone` fails.
  - `forge_route_of` taking a failed load as no route: the manifest-error test fails.
- **Round 1 findings:**
  - 1 and 2 are fixed. The fake route writes the tag into the bare origin with its own message, and the clone ends up with that tag on `tag_sha`. The second release from the same clone plans 0.1.2 from one commit.
  - 3 is fixed in the type shape: `ReleaseForge` with `ByClient` and `ByRoute`, and no `Option<&Path>` branch left in `run.rs`. The gap in its test coverage is finding 2.
  - 4 is fixed (`mod.rs:278`, `?`).
  - 6: the tickets are filed, but the citation is wrong (finding 5).
  - 7: the template is fixed; the rendered output is finding 1.
- **The `forge-route` script matches homma's calls:** it takes a fourth `message` argument for `tag`, and its git route makes the tag in the clone, so `fetch_tag` is a no-op on that route.
- **`ByClient`** does exactly what the release did before: tag with git, push the tag, `create_release`.
- **Every new test body** in `run_tests.rs`, `route_tests.rs` and `config.rs` was read, and each asserts something the code could get wrong.
- **Commit messages:** all eleven are lowercase imperative, under 72 characters, with no parentheses, no bylines and no bodies. No em-dashes anywhere in the diff.

## Notes

Findings 1 to 4 should be fixed before landing. Findings 2 and 3 fit in this open round: the src changelist is not locked yet, and 3 needs one line in the doc changelist. The Worker's release still carries no notes, so step 7's promise of the changelog block as the release body is not kept on the cloud path. That gap is ticketed (`forge_write_release_carries_no_notes_so_a_cloud_release_has`) and is not this branch's to close.
