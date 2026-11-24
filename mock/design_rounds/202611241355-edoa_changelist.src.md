# src changelist

Descends from `202611241355-edoa_topic.a-large-output-summarised-by-one-isolated-call.md` and the locked document changelist `202611241355-edoa_changelist.doc.lock.md`. Each new test carries a control that fails where its mechanism is taken out: the pass off, a model that fails, content one byte over a part, a summary line with no place.

## CHANGE: the manifest's tables FROM no `[summary]` TO `SummaryConfig` with `enabled`, `command` and `model` and their defaults, carried on `Config` as `summary`
File: `mock/crates/homma-api/src/config.rs`, `mock/crates/homma-api/src/lib.rs`, `mock/crates/homma-core/src/config.rs`
Verification: `SummaryConfig::default()` gives false, `claude` and `haiku`; a manifest with `[summary]` loads.

## CHANGE: the engine FROM no summarising pass TO `cmd::summary`: the rules and their version, the one-shot model trait and the command that implements it, the budgets and the parts, the count of summary lines with no place
File: `mock/crates/homma-engine/src/cmd/summary/mod.rs`, `mock/crates/homma-engine/src/cmd/summary/call.rs`, `mock/crates/homma-engine/src/cmd/summary/parts.rs`, `mock/crates/homma-engine/src/cmd/summary/tests.rs`, `mock/crates/homma-engine/Cargo.toml`
Verification: `the_pass_off_gives_the_output_whole_and_the_line_saying_so`, `a_failed_call_gives_the_output_whole_and_its_reason`, `content_over_one_request_is_cut_at_lines_into_parts_each_read_alone`, `every_line_goes_with_its_number_and_a_line_with_no_place_is_counted`, `a_brief_over_the_budget_is_not_sent`, `the_command_is_run_isolated_with_the_content_on_stdin` (a stand-in command script recording its arguments and directory).

## CHANGE: the command surface FROM no `summarise` TO `homma summarise --brief <text> [<file>]`, and `homma search --summarise`
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `mock/crates/homma-engine/src/cmd/search/mod.rs`, `mock/crates/homma-engine/src/cmd/search/tests.rs`, `mock/crates/homma-engine/tests/summarise_smoke.rs`, `README.md`
Verification: `homma summarise` prints the summary, or the text whole with the reason on stderr, exiting 0, through the binary with a manifest carrying `[summary]` and one without (`the_verb_prints_the_summary_with_the_pass_on_and_the_text_whole_with_it_off`); `homma search --summarise` goes through `search::answer`, generic over the model, which prints the summary in place of the hits and keeps the index lines, or the hits and the reason, and its JSON carries `summary` and `summary_note` (`a_summarised_search_prints_the_summary_in_place_of_the_hits`). The readme's command table names the verb.

## CHANGE: a call FROM ended by its own process alone, its pipes waited on without a bound TO run as the leader of its own process group, the group ended past the limit, the pipes read within what is left of it
File: `mock/crates/homma-engine/src/cmd/summary/call.rs`, `mock/crates/homma-engine/src/cmd/summary/tests.rs`
Verification: a stand-in command that exits leaving a child holding its output open is a failed call within the limit, in `the_command_is_run_isolated_with_the_content_on_stdin`. Found by the review pass.
