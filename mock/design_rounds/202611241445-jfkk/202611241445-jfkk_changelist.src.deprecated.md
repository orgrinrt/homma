# src changelist

Descends from `202611241445-jfkk_topic.a-question-answered-over-a-commands-output.md` and the locked document changelist `202611241445-jfkk_changelist.doc.lock.md`. Each new test carries a control that fails where its mechanism is taken out: a rule that does not match, a hook that allows, a chunk moved in the output, a changed chunk, a section missing from an answer.

## CHANGE: the settings' hook registrations FROM read only for a repository's hook files TO one reading of every event, matcher and command a settings file registers, which the aggregation and the ask's guard both use
File: `mock/crates/homma-engine/src/cmd/aggregate.rs`
Verification: the aggregation tests stay green; `registrations` is the one place a settings file's `hooks` table is walked.

## CHANGE: the engine FROM no question over a command's output TO `cmd::ask`: the guard, the run, the chunks and their digests, the two caches, the partial and answer calls
File: `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/guard.rs`, `mock/crates/homma-engine/src/cmd/ask/chunks.rs`, `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`, `mock/crates/homma-engine/src/cmd/summary/mod.rs`
Verification: `a_deny_rule_refuses_the_command_whole_or_in_part`, `a_bash_hook_that_blocks_refuses_the_command_and_one_that_allows_does_not`, `a_chunk_keeps_its_partial_wherever_it_moves_in_the_output`, `an_unchanged_output_and_question_reuse_the_answer_with_no_call`, `a_changed_chunk_alone_is_read_again`, `an_answer_missing_a_chunk_fails_the_ask`, `the_pass_off_answers_nothing_and_says_why`.

## CHANGE: the command surface FROM no `ask` TO `homma ask --run <command> <question>`, behind the `search` feature
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `README.md`
Verification: the verb parses; the readme's command table names it.
