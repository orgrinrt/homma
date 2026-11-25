# src changelist

Descends from `202611251121-mtgb_changelist.doc.lock.md`.

## CHANGE: FROM the `[surfaces]` table and homma's effort verdict TO none
File: `mock/crates/homma-api/src/surfaces.rs`, `mock/crates/homma-api/src/lib.rs`, `mock/crates/homma-core/src/config.rs`
Verification: `surfaces.rs` is deleted with `SurfacesConfig`, `ModesConfig`, `PrinciplesConfig` and `EffortGate`; `Config` has no `surfaces` field, so a manifest with `[surfaces]` is refused as before the branch.

## CHANGE: the renders FROM reading `[surfaces]` TO the caller's arguments
File: `mock/crates/homma-engine/src/cli.rs`, `mock/crates/homma-engine/src/cmd/mod.rs`, `mock/crates/homma-engine/src/cmd/skills.rs`, `mock/crates/homma-engine/src/cmd/rules.rs`
Verification: `SkillsOp::Render` takes `ModesArgs` (`--modes`, `--mode-prefix`, `--mode-body-max`, all or none) and `--no-effort`; `RulesOp::Render` and `RulesOp::Load` take `PrinciplesArgs` (`--principles`, `--principle-prefix`, `--principle-title`, all or none); `skills::render::run` keeps the effort keys and writes the modes unless `--no-effort`, where it strips and removes them.

## CHANGE: the ask cache and the what acceptance FROM changed here TO the trunk's source
File: `mock/crates/homma-api/src/config.rs`, `mock/crates/homma-api/src/lib.rs`, `mock/crates/homma-core/src/config.rs`, `mock/crates/homma-engine/src/cmd/ask/cache.rs`, `mock/crates/homma-engine/src/cmd/ask/mod.rs`, `mock/crates/homma-engine/src/cmd/ask/tests.rs`, `mock/crates/homma-engine/src/cmd/what/mod.rs`, `mock/crates/homma-engine/src/cmd/what/fixture.rs`, `mock/crates/homma-engine/src/cmd/what/accept_tests.rs`, `mock/crates/homma-engine/src/cmd/what/sources_tests.rs`
Verification: each reads as on `origin/dev`, `fixture.rs` deleted.

## CHANGE: FROM no command-layer tests of the kinds TO tests shown red first
File: `mock/crates/homma-engine/src/cmd/skills.rs`, `mock/crates/homma-engine/src/cmd/skills_tests.rs`, `mock/crates/homma-engine/src/cmd/rules.rs`, `mock/crates/homma-engine/src/cmd/rules_tests.rs`
Verification: `skills render` refuses a mode whose skill name an authored skill has; `--no-effort` removes an earlier mode skill and strips `effort` while the authored skill stays; with the flag absent `effort` is kept and the mode skill written; `rules render` writes `principle-<name>.md` from the named directory and `rules load` counts its card.
