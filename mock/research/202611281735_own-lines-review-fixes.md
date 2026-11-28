# Own lines: the review's fixes

Round 202611281719 on `work/homma-release-per-crate`, closed, answering
section 4 of the review at `.shared/research/202611281710_review-of-four-ready-branches/README.md`
on the workspace's `main`. Finding 2 there is void: op answered "Own line
stands".

## What changed

- Blocking 1: every release tag is checked as one. The check reads its own
  lines first and runs `tag.annotated`, `tag.reachable`, `tag.pushed`,
  `tag.sha.agrees` and `tag.local` over the repo's version tags and every own
  line's tag. `tag.dupes` groups by commit and line, so a release's one merge
  carrying `v0.1.1` and `t-p-v0.0.1-rc.1` is no duplicate, and two tags of one
  line on one commit still are.
- The lockfile: a commit's line is read off the paths it changed against its
  first parent. A change to the root `Cargo.lock` counts for the repo's line
  only where it changes a package on no own line, compared with the own lines'
  packages and the references to them taken out. Moving an own line's version
  by hand, lock and all, no longer refuses a release of own lines alone; a
  lock change to another package still does.
- Merge subjects: the same per-commit reading replaces the pathspec walk, so a
  pull request's merge, which is the same as its second parent for any path,
  stays in the repo's block with its number.

## Tests

In `lines_tests.rs`:
`an_own_tag_is_checked_as_a_release_tag_on_main_annotated_and_pushed`,
`the_repos_tag_beside_an_own_tag_on_one_merge_is_no_duplicate_and_two_of_one_line_are`
and `a_pull_requests_merge_keeps_its_subject_in_the_repos_block_beside_own_lines`
failed before the change and pass after.
`a_lock_change_that_only_follows_an_own_lines_move_is_that_lines_alone` passes,
with a control that a lock change of `t-s` refuses, and fails with the lock
always read as the repo's. Release tests 356 pass; `cargo mock test` green.
