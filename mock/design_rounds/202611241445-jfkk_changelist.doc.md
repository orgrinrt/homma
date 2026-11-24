# doc changelist

Descends from `202611241445-jfkk_topic.a-question-answered-over-a-commands-output.md`.

## CHANGE: the command surface FROM no question over a command's output TO `homma ask` in the list and a section naming the run, the session's checks, the chunks, the two caches and their home, the two kinds of call, and what reaches the caller
File: `mock/DESIGN.md.tmpl`
Verification: the command list has `homma ask`; the section "Asking a question over a command's output" names `--run`, the `search` feature, the deny rules of the three settings files matched whole and per command, the `PreToolUse` hooks for `Bash` with their payload and what refuses, the index crate's chunker and the chunk digest, the partial and answer keys, `homma/ask/` under the git directory, the partials' relative lines moved to the output's, the batched partial calls and the one answer call, the exit 1 with no answer, and the lines printed.

## CHANGE: the ask's chunks FROM the index crate's paragraph chunker, behind the `search` feature TO cut by content in the verb, a blank line, a boundary line's digest or the 8 192-byte cap ending a chunk, and the verb in every build
File: `mock/DESIGN.md.tmpl`
Verification: the section no longer names the `search` feature or the index crate's chunker; it names the three ways a chunk ends, the 512-byte minimum, blank lines in no chunk, the digest, and which cut is positional. Why: building on the index crate's chunker showed it packs paragraphs greedily from the start of the text up to its budget, so text added near the top moves every boundary after it and two short paragraphs are one chunk; that is a cut by position, which the cache cannot key on, against the topic's "Chunks are cut on content". The verb then needs nothing of the index crate. A content-defined cut offered by the index crate, shared by both, is a ticket.

## CHANGE: the engine's summary FROM no `ask` TO `ask` named among the families
File: `mock/crates/homma-engine/README.md.tmpl`
Verification: the summary has the clause on `ask`.
