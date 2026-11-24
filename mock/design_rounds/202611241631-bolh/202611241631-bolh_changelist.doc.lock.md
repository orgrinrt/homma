# doc changelist

Descends from `202611241631-bolh_topic.the-ask-reads-its-input-and-runs-no-command.md`, the round's one topic. Its points on the command, the long line and the cache key are the changes below. The insertion test and the ignored accept test are source only. The cache's keeping and expiry is the question `how_long_homma_ask_keeps_its_cached_answers_and_what_expires` and changes nothing here.

## CHANGE: `homma ask` FROM `--run <command>`, run once the session's deny rules and hooks let it TO the output piped to it on stdin, no command run and no guards of its own
File: `mock/DESIGN.md.tmpl`, `mock/crates/homma-engine/README.md.tmpl`, `README.md`
Verification: the section "Asking a question over a command's output" gives the form `<command> | homma ask <question>`, says the ask runs no command and the command is the caller's under the session's own rules and hooks, reads stdin as UTF-8 with any byte that is not replaced, and no longer describes `--run`, the settings files, the deny matching, the hooks or the ten-minute run; the crate summary and the root README's command table say the ask answers over output piped to it.

## CHANGE: a long line of output FROM one chunk however long TO pieces of at most 8 192 bytes, each a chunk on the line's number, cut as the summarising pass cuts a long line
File: `mock/DESIGN.md.tmpl`
Verification: the paragraph on the cut says a line of 8 192 bytes or more is cut into pieces of at most 8 192 bytes on character boundaries, each a chunk alone on the line's number, by the summarising pass's own cut, and that the positional cut meets such a line.

## CHANGE: the cache keys FROM the rules' version and the model TO the rules' version, the command that answers and the model
File: `mock/DESIGN.md.tmpl`
Verification: the paragraph on the two caches keys a partial on the chunk's digest, the rules' version, `command` under `[summary]` and the model, and an answer on the question, the rules' version, the command, the model and each chunk's digest and first line.
