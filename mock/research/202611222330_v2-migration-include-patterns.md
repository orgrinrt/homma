# The include patterns for homma's data-ref migration

Written 22 November 2026 from `git ls-files mock` on `origin/dev` 0b10cf6, with `mockspace` `dev` at 385773c as the engine pin.

## What `mock/` holds

1626 tracked files:

- `design_rounds/`: 1307 files in 249 closed round directories, no flat file.
- `research/`: 6 files.
- `test-history/`: 7 files, a store of its own that no data ref takes.
- `crates/`: `homma-api`, `homma-core`, `homma-engine`, `homma-org`, `homma-store`; `src`, `tests` and `Cargo.toml` stay, and 8 `*.md.tmpl` files sit among them.
- `mock/` itself: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` stay, and 3 `*.md.tmpl` files (`DESIGN`, `PRINCIPLES`, `WORKFLOW`) leave.
- `registry/`: absent. homma tracks no file under `mock/registry`.
- No `lints/`, `tools/` or `benches/` directory, no bench results, no legacy `YYYY-MM-DD_` round name, no `mock://` text.

The rendered readmes are `README.md` and `docs/*.md` at the repository root, outside `mock/`, and stay.

## The declaration

One namespace for the rest of `mock/`, the shape muisti runs today:

```toml
[[data_ref]]
namespace = "mock"
path = "."
include = ["research/**", "design_rounds/*/**", "crates/**/*.md.tmpl", "*.md.tmpl"]
```

The patterns match 1324 files: 6 research, 1307 round files, 8 crate templates and 3 top-level templates. Nothing else in `mock/` matches, so `test-history/`, the manifests and the toolchain file stay on the branch. A `path = "."` table takes no other `[[data_ref]]` beside it, so the registry namespace below is declared only when homma has a registry to move.

## The registry

`directory-to-ref` refuses a directory the branch tracks no file in, so there is no migration to run for homma's registry today. When homma gains a `mock/registry`, it is its own namespace made with the registry store's own command, not a pattern of this table, which refuses any pattern that could reach it.

## The commands, in order

```bash
cargo mock migrate directory-to-ref --namespace mock --path . \
  --include 'research/**' --include 'design_rounds/*/**' \
  --include 'crates/**/*.md.tmpl' --include '*.md.tmpl' \
  --form branch --dry-run
cargo mock migrate directory-to-ref --namespace mock --path . \
  --include 'research/**' --include 'design_rounds/*/**' \
  --include 'crates/**/*.md.tmpl' --include '*.md.tmpl' \
  --form branch
```

`--form branch` because the cloud proxy takes no custom ref. `references-to-letkajenkka` runs only where the dry run finds text to change, and `round-filenames` has nothing to rename. The migration waits for mockspace v2 on `dev`; no step above has been run.

Verification after it: `git ls-files mock/research mock/design_rounds` is empty, `cargo mock data-refs status` is clean, the 1324 files are on disk byte for byte, and `cargo mock lints` passes.
