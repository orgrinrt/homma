# src changelist

Descends from `202611251551-itcw_changelist.doc.lock.md`.

## CHANGE: the ask's cache tests FROM in `tests.rs` TO in `cache_tests.rs` beside it
File: `mock/crates/homma-engine/src/cmd/ask/tests.rs`, `mock/crates/homma-engine/src/cmd/ask/cache_tests.rs`, `mock/crates/homma-engine/src/cmd/ask/mod.rs`
Verification: the expiry, cap, write-in-flight, git-question and sweep-in-run tests and their helpers `aged`, `stamp` and `HOUR` live in `cache_tests.rs`; `tests.rs` keeps the rest and lends `cache`; both files are under 500 lines; the same tests pass.

## CHANGE: the header tests of `what` FROM in `sources_tests.rs` TO in `header_tests.rs` beside it
File: `mock/crates/homma-engine/src/cmd/what/sources_tests.rs`, `mock/crates/homma-engine/src/cmd/what/header_tests.rs`, `mock/crates/homma-engine/src/cmd/what/mod.rs`
Verification: the tests that call `scripts::parse` or `scripts::interpreter` on a text and plant nothing live in `header_tests.rs`; `sources_tests.rs` keeps the gathering tests; both under 500 lines; the same tests pass.
