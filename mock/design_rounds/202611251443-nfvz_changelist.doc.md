# doc changelist

Descends from `202611251443-nfvz_topic.what-a-review-of-the-test-fixtures-round-found.md`, the round's one topic. Its section on a write left behind changes the design below; its sections on the scope of the names, the planted script, the cache question and the small notes are source only, since the design says nothing of a test's words, a test helper or how a command is built.

## CHANGE: a dot file under the caches FROM never removed TO removed once older than the expiry, and never counted
File: `mock/DESIGN.md.tmpl`
Verification: the paragraph on the sweep says a dot file is never counted toward the cap and is removed only once it is older than the expiry, so a write in flight is left alone and one an ask left behind does not stay forever.
