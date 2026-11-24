# src changelist

Descends from `202611242203-jxpf_changelist.doc.lock.md`; each change below transcribes one of its two.

## CHANGE: the index crate FROM `dev` at b8c04b8 TO `dev` at 5bf5422, which records an index's definition and gives `layer::Index::behind` and `layer::Index::refresh`
File: `mock/Cargo.lock`
Verification: the `muisti` package's source names `#5bf54222bba582df681d8102c3c968500772e0d5`; nothing else in the lock moved.

## CHANGE: `SearchConfig` FROM four keys TO six, `corpus_build` and `member_build`
File: `mock/crates/homma-api/src/config.rs`
Verification: `SearchConfig` has `corpus_build: String` defaulting to `muisti index` and `member_build: String` defaulting to `cargo mock search --refresh`, each documented as the command an index not built is named with and where it runs.

## CHANGE: a place FROM its name, file, reason shut and notes TO those and the command that builds its index, said with where it runs
File: `mock/crates/homma-engine/src/cmd/search/discover.rs`
Verification: `Place` has `build: String`; a corpus's is `corpus_build` in the corpus's directory, a member's `member_build` in its clone; `Place::at` takes it.

## CHANGE: an index FROM asked as it stands TO brought current first, or measured and said behind
File: `mock/crates/homma-engine/src/cmd/search/current.rs` (new), `mock/crates/homma-engine/src/cmd/search/mod.rs`
Verification: `current.rs` holds `Currency` (`Refreshed` with added, changed, removed and commits; `Behind` with files, commits and why; `Unknown` with why) and `bring`, which refreshes an opened index with the embedder given and, where the model differs, the refresh fails or the file records no definition, measures it with `Index::behind` and says why. `ask` brings each opened place current before it goes into the question, one at a time; `IndexOut` carries the place's `Currency`, and its line prints it; a place whose index is not built is a line naming its `build`.

## CHANGE: tests of an index brought current, one of another model behind, one with no definition, and one not built named with its command
File: `mock/crates/homma-engine/src/cmd/search/tests.rs`
Verification: an index of the verb's model built through the walk and then given a new file is refreshed by the question and its hit is the new file, its line saying one added; with nothing moved the line says it was current (the control); an index of another model with a new file beside it is not refreshed, the new file is not a hit and its line says it is behind by one file and names its model; an index recording no definition says how far behind is not known; an index not built names the command that builds it.
