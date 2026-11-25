# src changelist

Descends from `202611250846-mzpu_changelist.doc.lock.md`.

## CHANGE: the days of a listing FROM a `u32` that `list` turned from zero into one TO a `NonZeroU32` from the flag down
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/search/mod.rs`, `mock/crates/homma-engine/src/cmd/search/fresh.rs`
Verification: `Command::Search::fresh`, `Asked::Fresh`, `fresh::list` and `fresh::answer` take `NonZeroU32`; `list` holds no `max(1)` and reports the window it was given; `fresh_parses_bare_with_a_number_and_refuses_words` still refuses `--fresh 0`.

## CHANGE: the refusal of words beside `--fresh` and the merge of the lists by day FROM unmarked TO each carrying a `FIXME(<ticket>)` marker
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/search/fresh.rs`
Verification: both markers name `muisti_s_layer_lists_what_a_filter_keeps_across_indexes_newe`, which covers a filter taken by the layer for a fused question and a listing merged by day.

## CHANGE: FROM the listing's tests TO those and the cases the review asked for
File: `mock/crates/homma-engine/src/cmd/search/tests_fresh.rs`, `mock/crates/homma-engine/src/cmd/search/tests_fresh_edges.rs`, `mock/crates/homma-engine/src/cmd/search/mod.rs`
Verification: `tests_fresh.rs` shares its planting helpers with `tests_fresh_edges.rs`, a module of its own so neither passes 500 lines; with failing controls: a commit at 23:30Z listed at 00:10Z the next day is `1 day ago` and outside a window of one day, inside one of two; a repository index built without its history lists its file and no commit, where one built with it lists the commit; a place missing and a place shut keep their lines and are not asked in a listing beside one that is; `age` is in the JSON of a listing's hits and not a question's; `--limit` bounds the hits the pass reads.
