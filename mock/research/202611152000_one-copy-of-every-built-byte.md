# One copy of every built byte: what cargo offers homma

The part of a workspace-wide reading that is about homma. The whole of it, with the plan, the measurements, the probe and the table of third-party tools, is in the clause-dev workspace under `.shared/research/202611152000_one-copy-of-every-built-byte/` (`README.md`, `cargo-and-rustc-options.md`, `third-party-tools.md`, `sketches/cargo-sharing-probe/`), and every source is saved there under `.data/fetched/` by the hash given here. Fetched and checked on 15 November 2026 against the pin `nightly-2026-05-28`, cargo 1.98.0-nightly (fbb61be30 2026-05-26).

Op asked that one of each built part be on disk and that sharing and cleanup happen inside the harness. Homma has two places in it: the launcher, which builds the engine into a target of its own, and `agent regen`, which builds a member in order to render text.

## The launcher's target

On the day measured, `~/.cache/homma/builds/<key>/target` held 557M, and 133 of its 242 crates were also in the workspace's shared target. Cargo's `build.build-dir` (`CARGO_BUILD_BUILD_DIR`) separates intermediate artifacts from final ones (Cargo Book, Configuration and Build Cache, 51789a6532a8 and 443474bb4343, read), needs no flag on the pin, and with one build directory under two target directories a crate compiled once and one rlib was kept (probe line 11, checked). The launcher can therefore keep its per-pin directory for the engine binary and pass the workspace's build directory for everything else, as long as it builds with the workspace's toolchain and flags, since cargo keeps a second artifact for a different toolchain or `RUSTFLAGS` wherever it builds (probe lines 6 and 9, checked). Plan items H1 to H3, which take the renki change of M1.

## Regenerating agent files without a build

`mock/crates/homma-engine/src/cmd/agent.rs:659` runs `cargo mock` with no arguments in each member. That is mockspace's build mode, which runs `cargo check` over the member's whole mock workspace before rendering; in kaski, 168 crates and a lockfile of 657 packages. It is started in the background by the workspace's `agent-regen` whenever a member's HEAD moved, and it takes the shared target's lock and the machine's compiler slots from whatever else is building. A red check also fails the regen: the workspace's last report read `kaski: homma agent regen failed: cargo_mock: cargo mock exited exit status: 1: cargo check failed`. The run was not timed, since timing it means building.

Mockspace has had `--generate-only` for the purpose, generation with no check and no lints, and it renders the agent files. Passing it from `invoke_cargo_mock` is the whole change, item H4 (ticket ccea24b2dcd8), with two tests in `cmd/agent/regen/tests.rs`: a member that does not compile still gets its rules, and a recording `cargo` on the path sees the flag.

## The third-party tools that touch homma

| Tool | What it does | Last release | Verdict for homma | Sources |
|---|---|---|---|---|
| cargo-binstall 1.27.0 | Installs a crate's binary from its GitHub release, compiling only as a fallback | 2026-11-11 | For third-party binaries in the workspace's stand-up; homma's own engine stays with the launcher, which already resolves it by pin | 790714a2fd0a, 6239a5d11e53 |
| sccache 0.19.0 | Compiler wrapper with its own cache | 2026-10-24 | Not adopted on one machine: a second store of the same bytes; the candidate if containers ever share builds through remote storage | 160bc047ccfd, e6a1becd258e |
| cargo-sweep 0.8.0 | Removes artifacts by age or by toolchain no longer installed | 2025-10-11, and its README says it is unmaintained | Wrapped by the workspace's `disk-trim`, not by homma | 83dde45c3856, 4723c0208f75 |
| cargo-deny 0.20.2 | Licences, advisories, bans, sources | 2026-07-09 | In use, and the shape the others are held against | b402540b9e3c |

holds for: cargo 1.98.0-nightly (fbb61be30 2026-05-26) where a line says checked, the nightly Cargo Book and the crates.io records of 15 November 2026 where it says read; homma at `origin/dev` of that day.
