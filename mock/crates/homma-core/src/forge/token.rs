//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where a forge client's credential comes from.
//!
//! Two sources, in order. An environment variable named by
//! [`ForgeConfig::token_env`], and a command named by
//! [`ForgeConfig::token_cmd`] that prints the token on stdout.
//!
//! The variable wins. It is the deliberate override, it is what a CI runner
//! sets, and a command reading a keychain cannot answer on a machine with no
//! session to unlock it.

use std::process::Command;

use crate::config::ForgeConfig;

/// The token for this forge, from whichever source has one.
///
/// `None` where neither does, which is anonymous access rather than an error:
/// a public repo answers without a credential, and the caller that needs one is
/// the caller that reports its absence.
pub fn resolve(forge: &ForgeConfig) -> Option<String> {
    from_env(forge).or_else(|| from_command(forge.token_cmd.as_deref()))
}

/// The variable's value, when it names one and that one is not empty.
///
/// An empty variable is treated as unset. Exporting a variable to the empty
/// string is how a shell says "not this one" and how a CI runner renders an
/// unset secret, and taking it literally sends an empty bearer token to a forge
/// that then answers exactly as it would to no credential at all.
fn from_env(forge: &ForgeConfig) -> Option<String> {
    forge
        .token_env
        .as_ref()
        .and_then(|var| std::env::var(var).ok())
        .filter(|v| !v.is_empty())
}

/// Run the command and take its first line.
///
/// The first line rather than the whole of stdout, because a tool that prints a
/// token is entitled to print a newline after it and some print a notice under
/// it. A trailing newline carried into an `Authorization` header is a header
/// the forge rejects for a reason nobody can see.
///
/// Silent on failure. A command that is missing, that exits non-zero, or that
/// prints nothing leaves the client anonymous, and the caller reports that as
/// the credential problem it is. Printing here would put a diagnostic in the
/// middle of whatever output the command the operator actually ran produces.
fn from_command(argv: Option<&[String]>) -> Option<String> {
    let (program, args) = argv?.split_first()?;
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
