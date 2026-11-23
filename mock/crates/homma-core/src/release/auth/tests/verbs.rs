//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The verbs that hand something out or correct it: env, token, expiry and diagnose.

use super::*;

#[test]
fn the_export_lines_read_each_token_through_the_verbs() {
    let store = Memory::with("crates-io", "t", None);
    store.put("npm", "n", None).unwrap();
    let lines = flow::env_lines(ENTRIES, &store, "homma release auth");
    assert_eq!(lines, [
        "export CARGO_REGISTRY_TOKEN=\"$(homma release auth token crates-io)\"",
        "# JSR_AUTH_TOKEN unset: nothing stored for jsr",
        "export NPM_CONFIG_USERCONFIG=\"$(homma release auth npmrc)\"",
    ]);
}

#[test]
fn a_token_is_handed_out_only_for_a_token_registry() {
    let store = Memory::with("crates-io", "secret", None);
    assert_eq!(flow::token(ENTRIES, &store, "crates-io").unwrap(), "secret");
    assert!(flow::token(ENTRIES, &store, "github").is_err());
    assert!(
        flow::token(ENTRIES, &store, "jsr")
            .unwrap_err()
            .contains("nothing stored")
    );
}

#[test]
fn an_expiry_is_corrected_without_the_token_changing() {
    let store = Memory::with("jsr", "keep-me", None);
    let said = flow::expiry(ENTRIES, &store, "jsr", "2026-12-03", today()).unwrap();
    assert_eq!(said, "  jsr: expiry 2026-12-03, 10 days left");
    assert_eq!(store.get("jsr").unwrap().unwrap().secret, "keep-me");
    assert_eq!(
        flow::expiry(ENTRIES, &store, "jsr", "never", today()).unwrap(),
        "  jsr: expiry none"
    );
    assert!(flow::expiry(ENTRIES, &store, "jsr", "01/01/2027", today()).is_err());
}

#[test]
fn the_diagnosis_shows_the_token_masked_and_what_the_registry_said() {
    let store = Memory::with("jsr", "abcdefghijklmnop", None);
    let out = flow::diagnose(ENTRIES, &store, &Canned((401, None)), "jsr", today())
        .unwrap()
        .join("\n");
    assert!(out.contains("16 chars, abcd...mnop"), "{out}");
    assert!(!out.contains("efghijkl"), "{out}");
    assert!(out.contains("http: 401"), "{out}");
    assert!(out.contains("verdict: invalid"), "{out}");
}
