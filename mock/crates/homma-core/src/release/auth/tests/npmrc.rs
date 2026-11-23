//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The npmrc npm reads a token from.

use std::path::Path;

use super::*;

fn npmrc_written(dir: &Path, secret: &str) -> std::path::PathBuf {
    let where_ = dir.join("npmrc");
    flow::write_npmrc(&where_, &Memory::with("npm", secret, None)).unwrap();
    where_
}

#[test]
fn the_line_is_the_one_npm_reads() {
    let d = tempfile::tempdir().unwrap();
    let at = npmrc_written(d.path(), "npm-token");
    assert_eq!(
        std::fs::read_to_string(at).unwrap(),
        "//registry.npmjs.org/:_authToken=npm-token\n"
    );
}

fn read_by_npm(path: &Path) -> String {
    let out = std::process::Command::new("npm")
        .args(["config", "list", "--userconfig"])
        .arg(path)
        .output()
        .expect("npm runs");
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

#[test]
fn npm_itself_recognises_the_key_we_wrote() {
    let d = tempfile::tempdir().unwrap();
    let said = read_by_npm(&npmrc_written(d.path(), "npm-token"));
    assert!(
        said.contains("//registry.npmjs.org/:_authToken = (protected)"),
        "{said}"
    );
    assert!(!said.contains("Unknown user config"), "{said}");
}

#[test]
fn a_misspelled_key_is_something_npm_complains_about() {
    let d = tempfile::tempdir().unwrap();
    let wrong = d.path().join("wrong");
    std::fs::write(&wrong, "//registry.npmjs.org/:_authTokenX=abc\n").unwrap();
    // npm 10 lists an unknown key as it does a known one, so the control is
    // that the misspelling is not read back as the key npm takes a token from
    let said = read_by_npm(&wrong);
    assert!(said.contains(":_authTokenX = (protected)"), "{said}");
    assert!(!said.contains(":_authToken = (protected)"), "{said}");
}

#[cfg(unix)]
#[test]
fn nobody_else_may_read_it() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let at = npmrc_written(d.path(), "npm-token");
    assert_eq!(
        std::fs::metadata(at).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_rotated_token_does_not_leave_the_old_one_underneath() {
    let d = tempfile::tempdir().unwrap();
    npmrc_written(d.path(), "first");
    let at = npmrc_written(d.path(), "second");
    let text = std::fs::read_to_string(at).unwrap();
    assert!(text.contains("second") && !text.contains("first"), "{text}");
}

#[test]
fn nothing_stored_writes_nothing_and_says_so() {
    let d = tempfile::tempdir().unwrap();
    let at = d.path().join("npmrc");
    let err = flow::write_npmrc(&at, &Memory::default()).unwrap_err();
    assert!(!at.exists());
    assert!(err.contains("npm"), "{err}");
}
