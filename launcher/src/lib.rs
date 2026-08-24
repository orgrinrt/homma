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
//! What lives here is the descriptor, and that is the whole crate. `mockspace`
//! needs four hooks beside its own, for a lint-rules dependency pinned to the
//! engine's revision, a durable git-hook gate, a retired alias to refuse and a
//! legacy lock pin. This tool needs none of them, which is the thing the
//! extraction was for.

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
    hooks:           Hooks::NONE,
};

/// The process entry, over the descriptor.
pub fn run_cli() -> ExitCode {
    // SAFETY: this is the process entry. Nothing else has run, so no other
    // thread exists to observe the environment renki scrubs.
    unsafe { renki::run(&TOOL) }
}
