# src changelist

Descends from `202611241721-znwy_topic.the-ask-says-what-it-prints-and-refuses-a-terminal.md` and the locked document changelist `202611241721-znwy_changelist.doc.lock.md`. Each new test carries a control that fails where its mechanism is taken out.

## CHANGE: the ask's input FROM stdin read whatever it is TO a terminal refused without reading
File: `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`
Verification: `ask::read_piped` takes whether the input is a terminal and returns no text without reading it when it is; `ask::run` passes `std::io::IsTerminal` of stdin and, given none, says nothing was piped to it and exits 1; a test gives it a terminal and a reader that fails the test if read (control: the same input not a terminal is read whole).

## CHANGE: the slow free-disk accept test FROM run every time TO ignored with its ticket
File: `mock/crates/homma-engine/src/cmd/what/accept_tests.rs`
Verification: `free_disk_finds_disk_trim_with_the_verb_that_matched` carries `#[ignore = "the_free_disk_accept_test_of_homma_what_runs_over_a_minute: ..."]`.
