# src changelist

Descends from `202611250003-qpms_changelist.doc.lock.md` and its topic's first three sections.

## CHANGE: a file with no definition FROM named with its command only where the refresh says so TO wherever the refresh or the measure says so; counts FROM always plural TO in their number
File: `mock/crates/homma-engine/src/cmd/search/current.rs`
Verification: `bring` adds `<build> writes one` to the line of any `Error::Undefined`, from `Index::refresh` or from `Index::behind`; the line says `1 file`, `1 commit` and `2 commits`.

## CHANGE: tests FROM a refresh that fails accepting either outcome TO pinning the one it gets; an index of another model with no definition; counts in their number
File: `mock/crates/homma-engine/src/cmd/search/tests_current.rs`
Verification: the first round's tests are in this file, not `tests.rs`, as its changelist said; the test of a refresh that fails asserts `Currency::Unknown` and the measure's words; a store of another model recording no definition, with a build command, names it on its line; `Currency::said` gives `1 file`, `2 files`, `1 commit` and `2 commits`.
