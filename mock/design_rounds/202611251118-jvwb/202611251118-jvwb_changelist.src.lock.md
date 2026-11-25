# src changelist

Descends from `202611251118-jvwb_changelist.doc.lock.md`.

## CHANGE: FROM the tests finding the workspace around the repository TO one planted workspace per test binary
File: `mock/crates/homma-engine/src/cmd/what/fixture.rs`, `mock/crates/homma-engine/src/cmd/what/mod.rs`
Verification: `fixture::workspace` plants, once per binary, two tools with verbs, scripts with banner headers and verbs, a sourced library, a test and a fixture script, a skill and a rule, all with invented, generic names, and returns its root; nothing in `what`'s tests walks up from the crate's directory.

## CHANGE: the acceptance set FROM queries over the live workspace TO the matcher's mechanisms over the planted one
File: `mock/crates/homma-engine/src/cmd/what/accept_tests.rs`
Verification: kept and rewritten over the planted workspace: a script found by its summary, the matched verb as the command, a misspelling found by its edit, a sourced library never offered, the footer naming what a kind filter left out, filler changing nothing, queries about nothing returning nothing, a query of only filler saying why, each answer saying where each word was found, the JSON's keys, a tool with verbs and no default never printed bare in the listing or the hits, the best answers naming what sits beside them. Deleted: every test that a particular tool or script of one workspace comes first, the ignored catalogue cases, the calibration, the default path's mockspace listing of the repositories around, and the earlier-message case `readme_tests` already plants.

## CHANGE: the four source tests reading the live tree FROM it TO the planted workspace
File: `mock/crates/homma-engine/src/cmd/what/sources_tests.rs`
Verification: a planted script's verbs and place, tests and fixtures not being scripts, the skills and rules read, a tool's verbs and bare command, each over `fixture::workspace`.
