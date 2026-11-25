# src changelist

Descends from `202611251443-nfvz_changelist.doc.lock.md`.

## CHANGE: a script a test runs FROM written in the test process by each helper TO written through one helper by a child `sh`
File: `mock/crates/homma-engine/src/cmd/planted.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `mock/crates/homma-engine/src/cmd/what/mock_tests.rs`, `mock/crates/homma-engine/src/cmd/summary/tests.rs`, `mock/crates/homma-engine/src/cmd/release/commands_tests.rs`, `mock/crates/homma-engine/src/cmd/answers/tests/mod.rs`, `mock/crates/homma-engine/src/cmd/agent/regen/tests.rs`, `mock/crates/homma-engine/src/cmd/gates/tests.rs`
Verification: `cmd::planted::script`, compiled for the tests only, writes a script and sets its mode in a child `sh`; `fake_cargo`, `stand_in`, `scanner`, the answers `script`, the regen stand-in and the gates stand-in call it and write no script of their own.

## CHANGE: the git question of `Cache::of_workspace` FROM built inline TO built by one function a test reads
File: `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: `git_dir_query` builds the command with `GIT_DIR` and `GIT_WORK_TREE` removed and `of_workspace` runs it; a test reads its environment and finds both removed, red with the removal taken out.

## CHANGE: the sweep FROM never removing a dot file TO removing one past the expiry, never counting it
File: `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: a fresh dot file larger than the cap stays and pushes no entry out; an old one is removed.

## CHANGE: the small notes
File: `mock/crates/homma-api/src/config/unreadable.rs`, `mock/crates/homma-engine/src/cmd/what/text_tests.rs`
Verification: the test-local doc comment links `Identity` again; the prefix test grades a four-letter prefix as a prefix.
