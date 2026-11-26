//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! What a test needs to stage the world outside the code under test.
//!
//! Taken only as a dev-dependency, and depending on nothing of homma's, so the
//! tests of any crate may take it.
//!
//! The tests of a binary run on many threads and each may fork. A script
//! written by the test process is open for writing while it is written, and a
//! fork on another thread in that moment carries the descriptor into its child
//! until the child's exec, so running the script then fails as busy. Written by
//! a child `sh`, the script is never open for writing in the test process at
//! all; and a file the code under test writes is written in a child by running
//! that test alone there.

use std::path::Path;
use std::process::{Command, Output};

/// Write `text` to `path`, creating its directory, and give it `mode`, all in a
/// child `sh`.
pub fn script(path: &Path, text: &str, mode: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let planted = Command::new("sh")
        .args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod \"$3\" \"$2\"", "sh"])
        .arg(text)
        .arg(path)
        .arg(format!("{mode:o}"))
        .status()
        .unwrap();
    assert!(planted.success(), "{} was not planted", path.display());
}

/// The command that runs the one test `name` of the current test binary alone,
/// in a child. `module` is the test's `module_path!()`.
pub fn alone(module: &str, name: &str) -> Command {
    let exact = match module.split_once("::") {
        Some((_, path)) => format!("{path}::{name}"),
        None => name.to_string(),
    };
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", &exact, "--test-threads", "1"]);
    cmd
}

/// Checks that a child [`alone`] started ran its one test and that it passed.
pub fn passed(out: &Output) {
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        said.lines()
            .any(|l| l.starts_with("test result: ok. 1 passed;")),
        "the child did not run its one test to a pass: {said}"
    );
}

#[cfg(test)]
mod tests;
