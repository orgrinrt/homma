//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The pin the launcher reads does not break the engine that reads the same
//! file afterwards.
//!
//! One file serves two programs. The launcher reads `homma_branch` and its
//! siblings to decide which engine to build; the engine then parses the whole
//! file with `deny_unknown_fields` and would reject the very key that selected
//! it. It did: adding the pin to a real workspace made every command fail with
//! `unknown field homma_branch`, after the launcher had already used it.
//!
//! Only this crate can hold both ends. The engine must not depend on its
//! launcher, and the launcher treats the config as text so it stays engine
//! agnostic, so neither can check the other alone.
//!
//! It also pins the root workspace's `exclude = ["mock"]`, at no cost: without
//! it cargo drags the engine's crates in as implicit members of a root that
//! declares no `workspace.package`, and this file stops compiling.
//!
//! **What this covers**: the five spellings the launcher reads today. It does
//! not cover a spelling `renki` might add later, because the set is derived
//! inside `Header::parse` and is not enumerable from out here. Tracked as
//! `renki-exposes-its-pin-suffixes` on the agenda.

use homma::TOOL;
use renki::Header;

/// A workspace config carrying one pin key and the minimum the engine needs.
fn config_with(key: &str, value: &str) -> String {
    format!("{key} = \"{value}\"\n\n[workspace]\nname = \"ws\"\n")
}

/// Every pin key the launcher reads, with a value of the shape it expects.
fn every_pin_key() -> Vec<(String, &'static str)> {
    let p = TOOL.pin_prefix;
    vec![
        (format!("{p}_rev"), "22272ce84950"),
        (format!("{p}_tag"), "0.1.0"),
        (format!("{p}_branch"), "dev"),
        (format!("{p}_version"), "0.1.0"),
        (format!("{p}_git"), "ssh://git@example.invalid/x.git"),
    ]
}

#[test]
fn the_engine_accepts_every_key_the_launcher_reads() {
    for (key, value) in every_pin_key() {
        let text = config_with(&key, value);
        let parsed = homma_core::Config::parse(&text);
        assert!(
            parsed.is_ok(),
            "the engine rejects `{key}`, which the launcher reads from the same \
             file: {:?}",
            parsed.err()
        );
    }
}

#[test]
fn the_launcher_reads_every_key_this_test_claims_it_does() {
    // The control on the list above. Without it the first test would pass by
    // asserting the engine tolerates five keys nobody looks at, which is a
    // statement about nothing.
    for (key, value) in every_pin_key() {
        let header = Header::parse(&TOOL, &config_with(&key, value));
        assert!(
            header.pin.is_some() || header.url.is_some(),
            "the launcher reads nothing from `{key}`, so this list is wrong"
        );
    }
}

#[test]
fn a_key_that_is_not_a_pin_is_still_rejected_by_the_engine() {
    // The other control. `deny_unknown_fields` is what makes a typo visible,
    // and the fix above must not have turned it off wholesale.
    let text = config_with("homma_brunch", "dev");
    assert!(
        homma_core::Config::parse(&text).is_err(),
        "a misspelled key parsed, so the engine no longer catches typos"
    );
}

#[test]
fn the_pin_the_workspace_actually_uses_parses_at_both_ends() {
    // `homma_branch` specifically, because that is what a workspace here
    // carries and what produced the failure.
    let text = config_with("homma_branch", "dev");

    let header = Header::parse(&TOOL, &text);
    assert!(header.pin.is_some(), "the launcher found no pin");

    let cfg = homma_core::Config::parse(&text).expect("the engine rejected it");
    assert_eq!(cfg.workspace.name, "ws");
}
