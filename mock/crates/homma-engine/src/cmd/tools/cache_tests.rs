//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The root of the shared cache is read as the launcher's own is: the tool's variable, then the XDG one,
//! then the platform's directory under the home directory.

use std::ffi::OsStr;
use std::path::PathBuf;

use super::cache::root_from;

fn os(s: &str) -> Option<&OsStr> {
    Some(OsStr::new(s))
}

#[test]
fn the_tools_own_variable_names_the_whole_path_and_wins() {
    assert_eq!(
        root_from(os("/own"), os("/xdg"), os("/home/u")).unwrap(),
        PathBuf::from("/own")
    );
}

#[test]
fn the_xdg_directory_takes_homma_under_it_when_the_own_variable_is_unset_or_empty() {
    for own in [None, os("")] {
        assert_eq!(
            root_from(own, os("/xdg"), os("/home/u")).unwrap(),
            PathBuf::from("/xdg/homma")
        );
    }
}

#[test]
fn the_platform_default_is_under_the_home_directory_and_named_homma() {
    let p = root_from(None, None, os("/home/u")).unwrap();
    assert!(p.starts_with("/home/u") && p.ends_with("homma"), "{p:?}");
}

#[test]
fn no_variable_and_no_home_is_refused_naming_what_was_missing() {
    let e = root_from(None, None, None).unwrap_err();
    assert!(e.contains("HOME"), "{e}");
}

#[test]
fn a_variable_that_is_not_text_is_refused_naming_it() {
    use std::os::unix::ffi::OsStrExt;
    let bad = OsStr::from_bytes(&[0xFF, 0xFE]);
    let e = root_from(Some(bad), None, os("/h")).unwrap_err();
    assert!(e.contains("HOMMA_CACHE") && e.contains("not text"), "{e}");
}
