//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a row may run: words, never a line of shell.

use super::*;

const SHOWN: &str = "acre/.claude/hooks/x.sh";

#[test]
fn a_program_that_is_a_line_of_shell_is_refused_and_words_are_kept() {
    for shell in [
        "FOO=1",
        "cd dir &&",
        "true;",
        "cat |",
        "(cd dir;",
        "`which bash`",
        "$(which bash)",
        "bash <",
        "\"bash\"",
        "$SHELL",
    ] {
        let got = program_words(shell, SHOWN);
        assert!(
            got.as_ref().is_err_and(|e| e.contains("a line of shell")),
            "`{shell}` was kept as a program: {got:?}"
        );
    }
    // The controls: a program, one with arguments carrying an `=`, and none.
    assert_eq!(program_words("bash", SHOWN), Ok(vec!["bash".to_string()]));
    assert_eq!(
        program_words("env A=1 node", SHOWN),
        Ok(vec!["env".into(), "A=1".into(), "node".into()])
    );
    assert_eq!(
        program_words("node --flag=x", SHOWN),
        Ok(vec!["node".into(), "--flag=x".into()])
    );
    assert_eq!(program_words("", SHOWN), Ok(Vec::new()));
}

#[test]
fn arguments_that_would_not_split_into_what_the_host_was_handed_are_refused() {
    for args in ["'a b'", "\"a b\"", "a; b", "$HOME", "a\\ b", "a | b"] {
        assert!(
            argument_words(args, SHOWN).is_err_and(|e| e.contains("would not be what the host")),
            "`{args}` was split into words"
        );
    }
    assert_eq!(
        argument_words("--quiet  check", SHOWN),
        Ok(vec!["--quiet".into(), "check".into()])
    );
    assert_eq!(argument_words("", SHOWN), Ok(Vec::new()));
}
