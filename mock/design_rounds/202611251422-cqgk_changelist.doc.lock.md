# doc changelist

Descends from `202611251422-cqgk_topic.tests-that-name-nothing-real-and-four-small-fixes.md`, the round's one topic. Its sections on the ask's cache being the root's and on the sweep leaving a write in flight change the design below; its sections on the sample text of the tests, the planted script, the principle's render error, the frontmatter key, the planted workspace and the configuration module are source only, since the design says nothing of a test's words, an error's path, the key matcher's indentation or a module's files, and the behaviour it states does not change.

## CHANGE: the ask's caches FROM under the workspace root's git directory TO under the root's own git directory whatever the caller's environment names
File: `mock/DESIGN.md.tmpl`
Verification: the paragraph on the two caches says the git directory is the root's own, asked of git with `GIT_DIR` and `GIT_WORK_TREE` removed, so an ask run from a hook that exports them keeps its cache where it always does.

## CHANGE: the sweep FROM every file under the caches TO the entries only, a write in flight left alone
File: `mock/DESIGN.md.tmpl`
Verification: the paragraph on the sweep says an entry is written beside its place under a dot name and renamed over it, and that the sweep neither counts nor removes a dot file, so one ask never takes another's write from under it.
