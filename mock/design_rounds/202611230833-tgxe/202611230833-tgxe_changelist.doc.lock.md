# doc changelist

Topic: `202611230833-tgxe_topic.release-review-fixes.md`.

## mock/crates/homma-core

- `DEEPDIVE_release.md.tmpl`, Credentials: after the paragraph on the store, a store that refuses writes is found
  out before a paste is asked for: the walk for that entry stops, names the refusal, which names the variable, and
  the entry counts as needing work, as does a proven token the store then refuses; `auth`, `auth fix` and
  `auth status` exit non-zero. A stored secret never appears in a `Debug` rendering.
- `DEEPDIVE_release.md.tmpl`, The version timeline: `versions tag` names a tag in the repo's own convention, bare
  where it has no tags yet, as this workspace's repos tag.
- `DEEPDIVE_release.md.tmpl`, The estate: `--json <path>` beside `jumps` or `missing` is refused with a reason and
  writes nothing.
- `CORE_RELEASE.md`, rendered from the template by `cargo mock`.

## mock/crates/homma-engine

None. The engine's `auth`, `estate` and `versions` verbs follow the core design above; its template is untouched.

## Root

- `README.md`, the `release` row of the command table: now names the publish credentials and the version and
  estate reports beside the gate and the release merge. A root document, outside the round's gate.

## Other flat topics

None beyond this round's one. The refs this branch cannot see were read: every flat topic on them is already
archived on `dev`.
