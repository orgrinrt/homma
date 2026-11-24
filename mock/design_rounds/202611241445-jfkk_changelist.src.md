# src changelist

Descends from `202611241445-jfkk_topic.a-question-answered-over-a-commands-output.md` and the locked document changelist `202611241445-jfkk_changelist.doc.lock.md`, replacing `202611241445-jfkk_changelist.src.deprecated.md`. Each new test carries a control that fails where its mechanism is taken out: a rule that does not match, a hook that allows, a chunk moved in the output, a changed chunk, a section missing from an answer, the pass off.

## CHANGE: the settings' hook registrations FROM read only for a repository's hook files TO one reading of every event, matcher and command a settings file registers, which the aggregation and the ask's guard both use
File: `mock/crates/homma-engine/src/cmd/aggregate.rs`
Verification: the aggregation tests stay green; `registrations` is the one place a settings file's `hooks` table is walked.

## CHANGE: the summarising pass's call FROM private to the pass TO its spawn and its bounded wait shared with the ask, which runs the command and the hooks through them, and a digest of several texts as one version
File: `mock/crates/homma-engine/src/cmd/summary/call.rs`, `mock/crates/homma-engine/src/cmd/summary/mod.rs`
Verification: the summary tests stay green; `drive` gives the exit status so a hook's exit 2 is read; `version_of` keys both caches.

## CHANGE: the engine FROM no question over a command's output TO `cmd::ask`: the guard, the run, the chunks cut by content and their digests, the two caches, the partial and answer calls
File: `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/guard.rs`, `mock/crates/homma-engine/src/cmd/ask/chunks.rs`, `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: `a_deny_rule_refuses_the_command_whole_or_in_part`, `a_bash_hook_that_blocks_refuses_the_command_and_one_that_allows_does_not`, `chunks_end_at_blank_and_boundary_lines_and_stay_put_when_text_is_inserted`, `a_chunk_keeps_its_partial_wherever_it_moves_in_the_output`, `an_unchanged_output_and_question_reuse_the_answer_with_no_call`, `a_changed_chunk_alone_is_read_again`, `an_answer_missing_a_chunk_fails_the_ask`.

## CHANGE: the command surface FROM no `ask` TO `homma ask --run <command> <question>`
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `mock/crates/homma-engine/tests/ask_smoke.rs`, `README.md`
Verification: through the binary, with a stand-in model and a fresh repository as the workspace, the verb prints the answer and the line saying where it came from, and again from the cache; with `[summary]` off it exits 1 saying why (`the_verb_answers_from_a_stand_in_model_and_again_from_the_cache_and_refuses_when_off`). The readme's command table names it.

## Comparison to deprecated changelist

### change: the settings' hook registrations from read only for a repository's hook files to one reading of every event, matcher and command a settings file registers, which the aggregation and the ask's guard both use

Unchanged.

### change: the engine from no question over a command's output to `cmd::ask`: the guard, the run, the chunks and their digests, the two caches, the partial and answer calls

The chunks are cut by content in `chunks.rs` instead of by the index crate's chunker, which packs paragraphs by position; a test for the cut is added. The sharing of the pass's call, which the deprecated text left unnamed, is its own change above. The pass-off check moved to the smoke test, since it is the verb's.

### change: the command surface from no `ask` to `homma ask --run <command> <question>`, behind the `search` feature

No longer behind the `search` feature, since the verb uses nothing of the index crate; the smoke test is added.
