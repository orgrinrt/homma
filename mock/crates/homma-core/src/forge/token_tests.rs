//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests for [`super::token`].

use super::*;
use crate::config::ForgeKind;

/// Each test owns a distinct variable name, so the suite stays safe to run in
/// parallel while touching process environment.
fn forge(env: Option<&str>, cmd: Option<&[&str]>) -> ForgeConfig {
    ForgeConfig {
        kind:      ForgeKind::Github,
        base_url:  "https://example.invalid".into(),
        api_url:   "https://example.invalid/api".into(),
        token_env: env.map(str::to_string),
        token_cmd: cmd.map(|c| c.iter().map(|s| s.to_string()).collect()),
    }
}

#[test]
fn the_variable_wins_over_the_command() {
    let var = "HOMMA_TEST_TOKEN_ENV_WINS";
    unsafe { std::env::set_var(var, "from-the-environment") };
    let f = forge(Some(var), Some(&["echo", "from-the-command"]));
    assert_eq!(resolve(&f).as_deref(), Some("from-the-environment"));

    // the control: with the variable gone the same config takes the command,
    // so the assertion above is about precedence rather than about the command
    // never having worked
    unsafe { std::env::remove_var(var) };
    assert_eq!(resolve(&f).as_deref(), Some("from-the-command"));
}

#[test]
fn an_empty_variable_is_not_a_credential_and_falls_through() {
    // Exporting a variable to the empty string is how a shell says "not this
    // one", and how a CI runner renders a secret it does not hold. Taking it
    // literally sends an empty bearer token, which a forge answers exactly as
    // it answers no credential, so the failure is invisible.
    let var = "HOMMA_TEST_TOKEN_ENV_EMPTY";
    unsafe { std::env::set_var(var, "") };
    let f = forge(Some(var), Some(&["echo", "from-the-command"]));
    assert_eq!(resolve(&f).as_deref(), Some("from-the-command"));
}

#[test]
fn only_the_first_line_of_the_commands_output_is_the_token() {
    // A tool that prints a token is entitled to print a newline after it, and
    // several print a notice under it. A trailing newline inside an
    // `Authorization` header is rejected for a reason nobody can see.
    let f = forge(None, Some(&["printf", "tok\\nnote: minted today\\n"]));
    assert_eq!(resolve(&f).as_deref(), Some("tok"));
}

#[test]
fn surrounding_whitespace_is_not_part_of_the_token() {
    let f = forge(None, Some(&["printf", "  tok  \\n"]));
    assert_eq!(resolve(&f).as_deref(), Some("tok"));
}

#[test]
fn a_first_line_carrying_a_space_is_not_a_token() {
    // No forge credential has a space in it. What does look like this is a
    // helper printing a prompt, or writing its error to stdout instead of
    // stderr, and sending either as a bearer token puts it in the process list
    // for a request the forge was always going to reject.
    assert_eq!(
        resolve(&forge(None, Some(&["printf", "Password: hunter2\n"]))),
        None
    );
    assert_eq!(
        resolve(&forge(None, Some(&["printf", "error: not logged in\n"]))),
        None
    );
    // A tab is whitespace too, and is what a helper printing columns emits.
    assert_eq!(
        resolve(&forge(None, Some(&["printf", "tok\tstale\n"]))),
        None
    );
}

#[test]
fn a_token_that_merely_looks_unusual_is_still_taken() {
    // The control for the check above. Real credentials carry punctuation, and
    // rejecting on anything broader than whitespace would refuse them.
    let f = forge(None, Some(&["printf", "ghp_aB3-x_9.z~qQ\n"]));
    assert_eq!(resolve(&f).as_deref(), Some("ghp_aB3-x_9.z~qQ"));
}

#[test]
fn a_command_that_fails_leaves_the_client_anonymous() {
    // `auth token <name>` exits 1 with its message on stderr when nothing is
    // stored, which is the ordinary case on a machine that has not minted one.
    assert_eq!(resolve(&forge(None, Some(&["false"]))), None);
}

#[test]
fn a_command_that_does_not_exist_leaves_the_client_anonymous() {
    assert_eq!(
        resolve(&forge(None, Some(&["homma-no-such-program-exists"]))),
        None
    );
}

#[test]
fn a_command_that_prints_nothing_is_not_an_empty_token() {
    // The distinction that matters: an empty string here would become an empty
    // bearer header rather than no header at all.
    assert_eq!(resolve(&forge(None, Some(&["true"]))), None);
    assert_eq!(resolve(&forge(None, Some(&["printf", "\\n\\n"]))), None);
}

#[test]
fn no_source_at_all_is_anonymous_rather_than_an_error() {
    // A public repo answers without a credential, so this is a real shape.
    assert_eq!(resolve(&forge(None, None)), None);
}

#[test]
fn an_empty_argument_list_is_not_a_program() {
    // A config carrying `token_cmd = []` must not try to run something.
    assert_eq!(resolve(&forge(None, Some(&[]))), None);
}

#[test]
fn a_registry_resolves_the_same_way_a_forge_does() {
    use crate::config::RegistryConfig;
    let var = "HOMMA_TEST_REGISTRY_TOKEN_A";
    // SAFETY: the test process owns its environment and no other test reads this name.
    unsafe { std::env::set_var(var, "from-env") };
    let reg = RegistryConfig {
        token_env: Some(var.into()),
        token_cmd: Some(vec!["echo".into(), "from-cmd".into()]),
    };
    assert_eq!(resolve_registry(&reg).as_deref(), Some("from-env"));
    unsafe { std::env::set_var(var, "") };
    assert_eq!(resolve_registry(&reg).as_deref(), Some("from-cmd"), "an empty variable falls through");
    unsafe { std::env::remove_var(var) };
    let none = RegistryConfig::default();
    assert_eq!(resolve_registry(&none), None);
}
