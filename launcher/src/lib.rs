//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma`: the launcher for the workspace harness engine.
//!
//! The launcher machinery is `renki` and is shared with other tools built the
//! same way: finding the root, reading the pin, building that engine once into
//! a per-version cache, execing it with an absolute working directory, keeping
//! itself current. None of that is this tool's and none of it lives here.
//!
//! What lives here is the descriptor, and one hook: the engine is not at the
//! top of this repo, so `--engine` pointed at a checkout of it needs to say
//! where. Everything else a tool can hook is left alone, which is the thing
//! the extraction was for.

use std::path::Path;
use std::process::ExitCode;

use renki::{Anchor, Cli, Hooks, Locate, Tool};

/// The canonical repository: the engine source when a config sets no
/// `homma_git`.
pub const CANONICAL_URL: &str = "ssh://git@github.com/orgrinrt/homma.git";

/// homma, as a launcher.
pub const TOOL: Tool = Tool {
    // The config sits above a pile of repositories rather than inside one, so
    // the anchor is the config itself. Anchoring on `.git` would stop at the
    // first member clone walking up and never reach the workspace root, and
    // running this from inside a member clone is how it is normally used.
    anchor:          Anchor::ConfigFile,
    short:           "homma",
    config_file:     "homma.toml",
    pin_prefix:      "homma",
    // The engine package, which is not the command. The command is this crate.
    engine_crate:    "homma-engine",
    cache_namespace: "homma",
    default_url:     CANONICAL_URL,
    launcher_crate:  "homma",
    // No working directory to descend into. The engine's own root is the
    // workspace root, which is where the config was found, so there is nothing
    // below it to name.
    workdir:         None,
    dir_flag:        Cli::DIR_FLAG,
    engine_flag:     Cli::ENGINE_FLAG,
    locate:          Locate::DEFAULT,
    hooks:           Hooks {
        verify_engine_dir: Some(engine_dir_holds_the_engine),
        ..Hooks::NONE
    },
};

/// The engine package inside a checkout of this repo.
pub const ENGINE_SUBDIR: &str = "mock/crates/homma-engine";

/// Refuse an `--engine` path that is not the engine's own package directory.
///
/// The engine sits well inside the tree rather than at the top of it, and the
/// top of the tree is what somebody reaches for. Left to cargo, that comes back
/// as a virtual manifest complaint naming neither the flag that caused it nor
/// the directory that would have worked.
fn engine_dir_holds_the_engine(dir: &Path) -> Result<(), String> {
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml"))
        .map_err(|e| format!("{}: {e}", dir.join("Cargo.toml").display()))?;
    if manifest.contains("name = \"homma-engine\"") {
        return Ok(());
    }
    let suggestion = dir.join(ENGINE_SUBDIR);
    if suggestion.join("Cargo.toml").is_file() {
        return Err(format!(
            "{} is the repository, not the engine package. Try {}",
            dir.display(),
            suggestion.display()
        ));
    }
    Err(format!(
        "{} is not a homma-engine checkout. The engine lives at {ENGINE_SUBDIR} inside one",
        dir.display()
    ))
}

/// The process entry, over the descriptor.
pub fn run_cli() -> ExitCode {
    // SAFETY: this is the process entry. Nothing else has run, so no other
    // thread exists to observe the environment renki scrubs.
    unsafe { renki::run(&TOOL) }
}
