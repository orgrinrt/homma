//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The descriptor is a pile of string literals and nothing else checks them.
//!
//! A rename that misses one compiles, ships, and fails at the moment an
//! operator runs the tool, with a message about a crate that does not exist.
//! These read the tree the strings claim to name.
//!
//! What is deliberately not here is the anchor's behaviour. Whether
//! `Anchor::ConfigFile` walks past a nested repository to reach the config is
//! `renki`'s question and `renki` answers it, in
//! `a_config_anchor_walks_past_a_nested_repo_to_reach_the_config`. Asserting it
//! again here would test the same code from further away.

use std::path::{Path, PathBuf};

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the launcher sits one directory under the repo root")
        .to_path_buf()
}

/// Every `name = "..."` in one TOML section, by section header.
///
/// A text scan rather than a parser, because pulling a TOML dependency into a
/// launcher whose whole point is having almost none would cost more than it
/// buys, and the two lines this reads are the two least likely in the file to
/// grow exotic syntax.
fn names_in_section(manifest: &str, header: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name") {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                out.push(v.trim().trim_matches('"').to_string());
            }
        }
    }
    out
}

#[test]
fn the_engine_crate_names_a_package_that_is_there() {
    let at = repo_root()
        .join("mock")
        .join("crates")
        .join(homma::TOOL.engine_crate)
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&at).unwrap_or_else(|e| {
        panic!(
            "the descriptor names engine_crate `{}`, and {} could not be read: {e}",
            homma::TOOL.engine_crate,
            at.display()
        )
    });
    assert_eq!(
        names_in_section(&manifest, "[package]"),
        vec![homma::TOOL.engine_crate.to_string()],
        "the package at {} is not the one the descriptor names",
        at.display()
    );
}

#[test]
fn the_engine_binary_is_named_for_the_crate_because_that_is_what_gets_run() {
    // `renki` execs the binary named by `engine_crate`. A package renamed
    // without its `[[bin]]` builds a binary under the old name and the launcher
    // then looks for one that was never produced.
    let at = repo_root()
        .join("mock")
        .join("crates")
        .join(homma::TOOL.engine_crate)
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&at).expect("engine manifest");
    assert_eq!(
        names_in_section(&manifest, "[[bin]]"),
        vec![homma::TOOL.engine_crate.to_string()],
        "the binary at {} is not named for the crate the launcher runs",
        at.display()
    );
}

#[test]
fn the_launcher_crate_is_this_one() {
    // The name `cargo install` takes. Wrong here and the self-update path
    // reinstalls something else, or nothing.
    assert_eq!(homma::TOOL.launcher_crate, env!("CARGO_PKG_NAME"));
}

#[test]
fn the_engine_and_the_launcher_do_not_share_a_name() {
    // The constraint the rename existed for, stated so it cannot come back.
    // Two binaries, one command name between them: whichever is installed last
    // wins and the other is unreachable.
    assert_ne!(homma::TOOL.engine_crate, homma::TOOL.launcher_crate);
}

#[test]
fn the_default_url_points_at_this_repository() {
    // Where the engine comes from when a config names no source. A url for
    // another repository resolves, builds, and runs the wrong tool.
    assert!(
        homma::TOOL.default_url.ends_with("/homma.git"),
        "default_url is {}",
        homma::TOOL.default_url
    );
}
