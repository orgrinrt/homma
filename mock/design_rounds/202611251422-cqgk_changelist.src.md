# src changelist

Descends from `202611251422-cqgk_changelist.doc.lock.md`.

## CHANGE: the sample text of the `what` tests FROM the names of one real tree TO invented ones
File: `mock/crates/homma-engine/src/cmd/what/sources_tests.rs`, `mock/crates/homma-engine/src/cmd/what/rank_tests.rs`, `mock/crates/homma-engine/src/cmd/what/readme_tests.rs`
Verification: the header, verb, readme and ranking tests check what they checked before over invented names: a garden's chores, a pigeon loft's messenger, a tidy-up tool; no script, tool, channel, branch or person of a real tree is named in the crate's `what` or `ask` tests.

## CHANGE: a script a test runs FROM written in the test process TO written by a child process
File: `mock/crates/homma-engine/src/cmd/what/mock_tests.rs`
Verification: `fake_cargo` writes the script through `sh`, so the test process never holds a descriptor open for writing on it; a test holds a write handle open on a script and shows the run refused as busy, and a script planted by `fake_cargo` runs.

## CHANGE: `Cache::of_workspace` FROM git with the caller's environment TO git with `GIT_DIR` and `GIT_WORK_TREE` removed
File: `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: the git question removes both variables; the sweep test removes them for its `git init` and asserts the cache sits under the planted root, and run with `GIT_DIR` exported at another repository it holds; with the removal taken out of `of_workspace` that run is red.

## CHANGE: the sweep FROM every file TO the entries only
File: `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: a file whose name begins with a dot is neither counted nor removed; a test plants an old `.<key>.<pid>` beside an old entry and only the entry goes.

## CHANGE: a rule FROM its name alone TO the path of the template it was read from
File: `mock/crates/homma-org/src/rules.rs`, `mock/crates/homma-org/tests/principles_corpus.rs`
Verification: `Rule` carries `source`, set where it is read; a render error names it; a principle whose card does not render is refused naming `principles/<name>.md.tmpl`, which exists.

## CHANGE: `frontmatter::without_key` FROM any line whose key matches TO unindented lines only
File: `mock/crates/homma-api/src/frontmatter.rs`
Verification: an indented line with the same key survives the removal and the unindented one does not.

## CHANGE: the planted workspace FROM kept in a static TO held by each test
File: `mock/crates/homma-engine/src/cmd/what/fixture.rs`, `mock/crates/homma-engine/src/cmd/what/accept_tests.rs`, `mock/crates/homma-engine/src/cmd/what/sources_tests.rs`
Verification: `fixture::workspace` returns the `TempDir` it planted; every caller holds it for the test; a test shows the root gone once the holder is dropped.

## CHANGE: `homma-api`'s configuration FROM one file past the limit TO a module tree
File: `mock/crates/homma-api/src/config.rs`, `mock/crates/homma-api/src/config/mod.rs`, `mock/crates/homma-api/src/config/identity.rs`, `mock/crates/homma-api/src/config/paths.rs`, `mock/crates/homma-api/src/config/tables.rs`, `mock/crates/homma-api/src/config/unreadable.rs`, `mock/crates/homma-api/src/lib.rs`
Verification: `config.rs` is gone; `config/mod.rs` holds `Workspace` and `LOCAL`, `identity` the role, staffing and identity, `paths` the paths, `tables` the search, summary and ask tables, `unreadable` the check for unreadable strings, each file under the limit with its tests beside it; the crate root's exports name the new places and no re-export is added; every test of the crate passes unchanged.
