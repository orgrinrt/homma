# A member on a version line of its own

Round 202611281607, branch `work/homma-release-per-crate`, closed.

## What landed

- A cargo member whose directory holds `release.toml` with
  `shared-version = false` keeps a line of its own: tags
  `<crate>-v<version>`, its version set by hand on `dev`, one step above its
  last own tag or its first release as it stands, its block in its own
  `CHANGELOG.md` and its own forge release. A primary declared so refuses
  (`man.primary`), and a changed own line that did not move refuses the plan,
  since `main` would carry it unreleased.
- The repo's line is the commits outside the own lines' directories. A run
  with a level or a version releases it and every moving own line in one bump
  and one merge, each tag on that merge. A named run with neither,
  `homma release run <repo>`, releases the own lines alone where the repo's
  line has nothing, and refuses with the count where it has something.
- The bump and merge are named by the release's name, so a stopped run of own
  lines alone is read back and not bumped twice.
- The check reads each own line against its own tags, leaves them out of the
  repo-tag rows, reads `main.unreleased` against the closest tag of any line,
  and passes over own tags in `tag.stray`.

## Where tomlfuse stands with it

`homma release plan tomlfuse` with this engine refuses: the repo's own line
has 10 commits since `v0.0.4`. `plan tomlfuse --version 0.0.5` plans
`tomlfuse` and `tomlfuse-core` at `0.0.5`, tagged `v0.0.5`, and
`tomlfuse-parse` new on its own line at `0.0.1-rc.1`, tagged
`tomlfuse-parse-v0.0.1-rc.1`, in one run. So releasing `tomlfuse-parse`
alone is not possible while `tomlfuse` has unreleased commits on `dev`: the
rule that `main` holds nothing unreleased makes the two go together. That is
the shape of option a in
`tomlfuse_parse_s_first_release_its_own_version_line_or_the_w`, which is
still op's to pick.

## Tests

`lines_tests.rs` (11) and `run_lines_tests.rs` (4) in `homma-core`, and
`a_named_repo_with_no_level_releases_its_own_lines_and_a_sweep_names_a_level`
in the engine. The check's own-line test failed before the check read own
lines. Reading the release name as a version only, and tagging the first tag
alone, each failed three of the four run tests. `cargo mock test` green; the
`homma-core` release tests 352 and the engine 831 pass.

## Left open

- The sweep across every repo still takes one level and has no way to release
  own lines alone.
- `work/identity-lane-2` holds an unlanded round, 202611231720, with a locked
  src changelist, five days old; this round did not take its topic in, since
  it is another body of work.
- The round's end tag has no route, `a_closed_design_round_s_end_tag_has_no_route`.
