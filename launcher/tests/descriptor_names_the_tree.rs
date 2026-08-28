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

/// Every `name` an array-of-tables section of a manifest declares.
///
/// A parse rather than a scan, for the reason `renki::package_name` gives: a
/// text scan accepts a package under one name declaring a `[[bin]]` under
/// another, and refuses a manifest that spells an assignment differently from
/// the one spelling it knows. The `[package]` case is `renki`'s and is used
/// through it; this covers `[[bin]]`, which it does not answer.
fn names_in_array_section(manifest: &str, section: &str) -> Vec<String> {
    let doc: toml::Value = toml::from_str(manifest).expect("the manifest does not parse");
    doc.get(section)
        .and_then(toml::Value::as_array)
        .map(|xs| {
            xs.iter()
                .filter_map(|x| x.get("name").and_then(toml::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The repository root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the launcher sits one directory under the repo root")
        .to_path_buf()
}

#[test]
fn the_engine_crate_names_a_package_that_is_there() {
    let at = repo_root()
        .join("mock")
        .join("crates")
        .join(homma::TOOL.engine_crate)
        .join("Cargo.toml");
    let dir = at.parent().expect("the manifest sits in a directory");
    assert_eq!(
        renki::package_name(dir).as_deref(),
        Ok(homma::TOOL.engine_crate),
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
        names_in_array_section(&manifest, "bin"),
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

/// Every scheme `git` will speak that needs no credential of the caller's.
///
/// `https` and `git` (the daemon protocol). Not `ssh`, which is the one that
/// reads as ordinary in a shell on the machine the repository was written on
/// and is a permission error everywhere else.
const ANONYMOUS: &[&str] = &["https://", "git://"];

#[test]
fn the_default_engine_url_is_one_a_stranger_can_fetch() {
    // The url in the descriptor is what an installed launcher hands to
    // `git ls-remote` and to `cargo install --git`, on a machine with no key
    // registered against this forge. An `ssh://` url there is not a slow path
    // or a fallback: it is the whole tool failing on first run for everybody
    // who is not the author.
    let url = homma::CANONICAL_URL;
    assert!(
        ANONYMOUS.iter().any(|s| url.starts_with(s)),
        "the default engine url is not anonymously fetchable: {url}"
    );
    assert!(
        !url.starts_with("ssh://"),
        "the default engine url wants a key: {url}"
    );
    assert!(
        !url.starts_with("git@"),
        "the default engine url wants a key: {url}"
    );
}

#[test]
fn the_check_above_can_fail() {
    // Both halves. A needle list that matched everything, or a check that read
    // an empty string, would pass the test above over any url at all.
    for bad in ["ssh://git@github.com/o/r.git", "git@github.com:o/r.git", ""] {
        assert!(
            !ANONYMOUS.iter().any(|s| bad.starts_with(s)),
            "an unfetchable url reads as fetchable: {bad}"
        );
    }
    assert!(
        ANONYMOUS
            .iter()
            .any(|s| "https://example.invalid/r.git".starts_with(s))
    );
}

#[test]
#[ignore = "reaches the network; run with --ignored to check the url actually resolves"]
fn the_default_engine_url_resolves_with_no_credentials() {
    // The offline check above reads a scheme. This one asks the forge, with
    // every credential path shut off, which is the state a stranger's machine
    // is in.
    let out = std::process::Command::new("git")
        .args(["ls-remote", homma::CANONICAL_URL, "HEAD"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o IdentityAgent=none",
        )
        .output()
        .expect("git could not be run");
    assert!(
        out.status.success(),
        "the default engine url does not resolve without credentials: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
