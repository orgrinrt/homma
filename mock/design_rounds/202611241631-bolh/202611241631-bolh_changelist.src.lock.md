# src changelist

Descends from `202611241631-bolh_topic.the-ask-reads-its-input-and-runs-no-command.md` and the locked document changelist `202611241631-bolh_changelist.doc.lock.md`. Each new test carries a control that fails where its mechanism is taken out.

## CHANGE: `homma ask` FROM `--run <command>` through the guards and a bounded run TO the question alone, over stdin, with no guards and no run
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/guard.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`, `mock/crates/homma-engine/tests/ask_smoke.rs`
Verification: `Command::Ask` has only `question`; `ask::run` reads stdin through `ask::read_input`, which decodes lossily; `guard.rs`, `run_command` and `RUN_LIMIT` are gone with their tests; a test reads bytes that are not UTF-8 with one replaced (control: UTF-8 comes through as it is); the smoke test pipes the output into the binary.

## CHANGE: a long line FROM one chunk however long TO pieces of at most `MAX` bytes, by the summarising pass's cut
File: `mock/crates/homma-engine/src/cmd/summary/parts.rs`, `mock/crates/homma-engine/src/cmd/ask/chunks.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: `summary::parts::cut_at_chars` splits text into pieces of at most a given number of bytes on character boundaries, and both `pieces` and `chunks::cut` use it; a test cuts a line of three and a half times `MAX` bytes of wide characters into four chunks on the line's number, each at most `MAX` bytes and valid UTF-8, and every batch of them fits the request (control: a line under `MAX` is one chunk).

## CHANGE: the cache keys FROM the model TO the command and the model
File: `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: `cache::Asker` holds the command and the model, and `partial_key` and `answer_key` take it; a test asks the same model through another command and reads every chunk again (control: the same command and model reuse the answer).

## CHANGE: the insertion test FROM one insertion at the top allowing two changed chunks TO insertions at the top, the middle and the end, each changing exactly one chunk
File: `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: for each of the three positions the chunks before and after are compared and exactly one is gone.

## CHANGE: the slow accept test FROM run every time TO ignored with its ticket
File: `mock/crates/homma-engine/src/cmd/what/accept_tests.rs`
Verification: `has_op_answered_this_before_finds_asked_before` carries `#[ignore = "the_asked_before_accept_test_of_homma_what_runs_over_a_minut: ..."]`.
