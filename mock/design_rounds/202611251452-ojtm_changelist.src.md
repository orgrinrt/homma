# src changelist

Descends from `202611251452-ojtm_changelist.doc.lock.md`.

## CHANGE: the regen stub launcher FROM written in the test process TO planted through `cmd::planted::script`
File: `mock/crates/homma-engine/src/cmd/agent/regen/tests.rs`
Verification: `stub_launcher` calls `crate::cmd::planted::script` and writes no file itself; no helper in the regen tests writes a script of its own; the regen tests pass.
