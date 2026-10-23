//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The four commands that reach a tool, answered where renki does not build.
//!
//! renki is unix only, and the tools run on it, so off unix each of these refuses with the reason
//! [`super::unix_only`] gives instead of the engine failing to build there.

use std::ffi::OsString;
use std::path::Path;

use anyhow::Result;

use super::{Asked, unix_only};
use crate::cli::OutputFormat;
use crate::cmd::Outcome;

/// `homma tools`.
///
/// # Errors
///
/// Always: see [`unix_only`].
pub fn list(_root: &Path, _format: OutputFormat) -> Result<Outcome> {
    Err(unix_only())
}

/// `homma tools check`.
///
/// # Errors
///
/// Always: see [`unix_only`].
pub fn check(_root: &Path, _format: OutputFormat) -> Result<Outcome> {
    Err(unix_only())
}

/// `homma tools run` and `homma <tool>`.
///
/// # Errors
///
/// Always: see [`unix_only`].
pub fn run_tool(_root: &Path, _name: &str, _words: &[OsString], _asked: Asked) -> Result<Outcome> {
    Err(unix_only())
}

/// A group's word handed on.
///
/// # Errors
///
/// Always: see [`unix_only`].
pub fn run_merged(_root: &Path, _group: &str, _words: &[OsString]) -> Result<Outcome> {
    Err(unix_only())
}
