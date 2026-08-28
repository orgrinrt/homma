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

use renki::{Anchor, Hooks, Locate, Tool, pin_keys};

/// The canonical repository: the engine source when a config sets no
/// `homma_git`.
///
/// Over https, because this is the url every installed launcher hands to
/// `git ls-remote` and `cargo install --git` on a machine that is not the
/// author's. The repository is public and an ssh url turns that into a
/// permission error for anybody without a key registered on the forge, which
/// is a failure on first run rather than a failure at some later step. A
/// consumer who does want ssh sets `homma_git` and gets it.
pub const CANONICAL_URL: &str = "https://github.com/orgrinrt/homma.git";

/// homma, as a launcher.
pub const TOOL: Tool = Tool {
    // The config sits above a pile of repositories rather than inside one, so
    // the anchor is the config itself. Anchoring on `.git` would stop at the
    // first member clone walking up and never reach the workspace root, and
    // running this from inside a member clone is how it is normally used.
    anchor: Anchor::ConfigFile,
    short: "homma",
    config_file: "homma.toml",
    pin_keys: pin_keys!("homma"),
    // The engine package, which is not the command. The command is this crate.
    engine_crate: "homma-engine",
    cache_namespace: "homma",
    default_url: CANONICAL_URL,
    launcher_crate: "homma",
    // No working directory to descend into. The engine's own root is the
    // workspace root, which is where the config was found, so there is nothing
    // below it to name.
    workdir: None,
    locate: Some(Locate::DEFAULT),
    hooks: Hooks {
        verify_engine_dir: Some(engine_dir_holds_the_engine),
        ..Hooks::NONE
    },
    // The flags, the retention, the skip list and the self-update policy are
    // renki's conventions and this tool wants all of them. Spread rather than
    // restated, so a field added to the descriptor arrives as a version bump
    // instead of a build break.
    ..Tool::CONVENTIONS
};

/// The descriptor is answerable at build time, so it is answered here.
///
/// `..Tool::CONVENTIONS` is a base of empty names, which is safe only because
/// `Tool::defect` refuses every one of them and is const. Writing the literal
/// out gave this for free: a missing field was a missing field. The spread
/// trades that away, and this one line buys it back.
const _: () = assert!(TOOL.defect().is_none());

/// The engine package inside a checkout of this repo.
pub const ENGINE_SUBDIR: &str = "mock/crates/homma-engine";

/// Refuse an `--engine` path that is not the engine's own package directory.
///
/// The engine sits well inside the tree rather than at the top of it, and the
/// top of the tree is what somebody reaches for. Left to cargo, that comes back
/// as a virtual manifest complaint naming neither the flag that caused it nor
/// the directory that would have worked.
fn engine_dir_holds_the_engine(dir: &Path) -> Result<(), String> {
    // `renki::package_name` rather than a reader of this crate's own. There was
    // one here, a text scan, and it had the defect renki's documentation names:
    // it knew one of TOML's spellings of an assignment and refused a manifest
    // written in another, so `[ package ]` with spaces was reported as not
    // being this engine at all. Reading the document as a document is what
    // makes that go away, and renki already parses TOML for the build registry.
    let declared = renki::package_name(dir);
    if declared.as_deref() == Ok(TOOL.engine_crate) {
        return Ok(());
    }
    // A directory with no manifest at all is a different mistake from one
    // holding the wrong package, and the reader already says which file it
    // could not read. Passing that through is what tells somebody who pointed
    // the flag at an empty directory what was actually looked for.
    if !dir.join("Cargo.toml").is_file() {
        return Err(declared
            .err()
            .unwrap_or_else(|| format!("{} holds no Cargo.toml", dir.display())));
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
