# doc changelist

Descends from `202611242203-jxpf_topic.search-brings-each-index-current.md`, the round's one topic. Each of its four sections is a change below.

## CHANGE: an index's currency FROM its owner's to say, homma saying only when it was last written TO brought current before it is asked, or its line saying how far behind it is and why
File: `mock/DESIGN.md.tmpl`
Verification: the `homma search` section says each index whose file opens is brought current through the index crate's `Index::refresh` with the verb's embedder before it is asked, its line saying what was added, changed and removed and the commits read, or that it was current; that an index of another model, one whose refresh is refused or fails, and one whose file records no definition are asked as they stand, each line saying how far behind it is by the crate's `Index::behind` and why, or that it is not known; that an index not built is not asked and its line names the command that builds it; that the verb never builds an index from nothing; and that when the file was last written is still said.

## CHANGE: the `[search]` table FROM four keys TO six, `corpus_build` and `member_build` added
File: `mock/crates/homma-api/DESIGN.md.tmpl`
Verification: the `SearchConfig` paragraph names `corpus_build`, `muisti index` by default and run in the corpus's directory, and `member_build`, `cargo mock search --refresh` by default and run in the member's clone, as the commands an index not built is named with.
