//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The one way a test plants a script it will run.
//!
//! The tests of this binary run on many threads and each may fork. A script
//! written by this process is open for writing while it is written, and a fork
//! on another thread in that moment carries the descriptor into its child until
//! the child's exec, so running the script then fails as busy. Written by a
//! child `sh`, the script is never open for writing in this process at all.

#![cfg(test)]

use std::path::Path;

/// Write `text` to `path`, creating its directory, and give it `mode`, all in a
/// child `sh`.
pub fn script(path: &Path, text: &str, mode: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let planted = std::process::Command::new("sh")
        .args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod \"$3\" \"$2\"", "sh"])
        .arg(text)
        .arg(path)
        .arg(format!("{mode:o}"))
        .status()
        .unwrap();
    assert!(planted.success(), "{} was not planted", path.display());
}
