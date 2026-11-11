//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The suggestion is made for a prefix and for a small slip, and only where one name is the closest.

use super::suggest::nearest;

const NAMES: [&str; 6] = ["checkup", "status", "release", "rules", "repo", "msg"];

#[test]
fn a_word_that_starts_a_name_is_that_name() {
    assert_eq!(nearest("check", NAMES), Some("checkup"));
    assert_eq!(nearest("statuses", NAMES), Some("status"));
}

#[test]
fn a_small_slip_is_the_name_it_slipped_from() {
    assert_eq!(nearest("stauts", NAMES), Some("status"));
    assert_eq!(nearest("relase", NAMES), Some("release"));
    assert_eq!(
        nearest("mgs", NAMES),
        None,
        "two edits on three letters is not close"
    );
}

#[test]
fn a_word_near_nothing_gets_nothing() {
    assert_eq!(nearest("frobnicate", NAMES), None);
    assert_eq!(nearest("xy", NAMES), None);
}

#[test]
fn two_equally_close_names_give_no_guess_and_the_same_name_twice_is_one() {
    assert_eq!(
        nearest("re", ["repo", "rules"]),
        None,
        "under three letters is no prefix"
    );
    assert_eq!(nearest("rep", ["repo", "report"]), None);
    assert_eq!(nearest("rep", ["repo", "repo"]), Some("repo"));
}

#[test]
fn a_prefix_beats_an_edit_and_the_word_itself_is_no_suggestion() {
    assert_eq!(nearest("stat", ["status", "stag"]), Some("status"));
    assert_eq!(nearest("status", ["status"]), None);
}

#[test]
fn case_is_not_a_difference() {
    assert_eq!(nearest("Check", NAMES), Some("checkup"));
}
