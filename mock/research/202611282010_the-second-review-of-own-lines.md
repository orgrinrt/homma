# The second review of own lines

Round 202611281948, closed at c01df5e on `work/homma-release-per-crate`.

- `first_parent_touching` has its doc back and `changed_paths` is gone with the rest of the per-commit reading.
- `subjects_touching` and its pathspec are gone; `subjects` is `subjects_to` over `from..to`.
- `subjects_with_paths` reads a range's commits, each with its paths against its first parent, in one `git log --diff-merges=first-parent --name-only`; `commits_where` reads through it, one process a line instead of two a commit. The deep dive says one walk of the log a line.
- `an_own_tag_is_checked_as_a_release_tag_on_main_annotated_and_pushed` asserts `tag.sha.agrees` on an own tag moved after its push and `tag.local` on an own tag the remote holds alone.
- `a_dependency_on_an_own_line_carrying_its_version_follows_that_lines_move`: a lock whose `t` depends on `t-p 0.0.1-rc.1`, as cargo writes one where two versions are locked; it plans `t-p` alone, and fails with the dependency `retain` in `lines.rs` made to keep every entry.
- `cargo mock test -p homma-core`: 789 tests green.
