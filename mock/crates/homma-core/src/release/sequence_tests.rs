//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The version laws, each paired: a case that must be caught and one that
//! must not be, so a guard cannot pass by reporting everything.

use super::*;

fn v(s: &str) -> Version {
    s.parse().unwrap()
}

fn vs(list: &[&str]) -> Vec<Version> {
    list.iter().map(|s| v(s)).collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(Version, Version)> {
    list.iter().map(|(a, b)| (v(a), v(b))).collect()
}

#[test]
fn rejects_what_is_not_a_semver() {
    for bad in ["", "1.0", "1.0.0.0", "1.0.x", "latest"] {
        assert!(bad.parse::<Version>().is_err(), "{bad}");
    }
    // a tag name reads with its `v`, which the python's bare parser refused
    assert_eq!(v("v1.0.0"), Version::new(1, 0, 0));
}

#[test]
fn accepts_release_and_prerelease() {
    assert_eq!(v("1.2.3"), Version::new(1, 2, 3));
    assert_eq!(v("1.2.3-alpha.1").prerelease.as_deref(), Some("alpha.1"));
}

#[test]
fn prerelease_sorts_below_its_own_release() {
    let mut l = vs(&["0.2.0", "0.2.0-alpha.1"]);
    l.sort();
    assert_eq!(l, vs(&["0.2.0-alpha.1", "0.2.0"]));
}

#[test]
fn prerelease_sorts_above_the_previous_release() {
    let mut l = vs(&["0.2.0-alpha.1", "0.1.9"]);
    l.sort();
    assert_eq!(l, vs(&["0.1.9", "0.2.0-alpha.1"]));
}

#[test]
fn numeric_not_lexical() {
    let mut l = vs(&["0.1.10", "0.1.9"]);
    l.sort();
    assert_eq!(l, vs(&["0.1.9", "0.1.10"]));
}

#[test]
fn the_three_legal_steps_are_not_jumps() {
    assert_eq!(gaps(&vs(&["1.0.0", "1.0.1"])), vec![]);
    assert_eq!(gaps(&vs(&["1.0.1", "1.1.0"])), vec![]);
    assert_eq!(gaps(&vs(&["1.1.0", "2.0.0"])), vec![]);
}

#[test]
fn a_skipped_patch_is_a_jump() {
    assert_eq!(gaps(&vs(&["0.1.1", "0.1.3"])), pairs(&[("0.1.1", "0.1.3")]));
}

#[test]
fn a_skipped_minor_is_a_jump() {
    assert_eq!(
        gaps(&vs(&["0.1.10", "0.3.0"])),
        pairs(&[("0.1.10", "0.3.0")])
    );
}

#[test]
fn prereleases_do_not_create_a_jump() {
    assert_eq!(gaps(&vs(&["0.2.0", "0.3.0-alpha.1", "0.3.0"])), vec![]);
}

#[test]
fn prereleases_do_not_hide_a_jump() {
    assert_eq!(
        gaps(&vs(&["0.1.0", "0.3.0-alpha.1", "0.3.0"])),
        pairs(&[("0.1.0", "0.3.0")])
    );
}

#[test]
fn unsorted_input_gives_the_same_answer() {
    assert_eq!(gaps(&vs(&["0.1.3", "0.1.1"])), pairs(&[("0.1.1", "0.1.3")]));
}

#[test]
fn a_non_gap_fills_with_nothing() {
    assert_eq!(bridge(&v("1.0.0"), &v("1.0.1"), &[]), Some(vec![]));
}

#[test]
fn a_gap_names_what_closes_it() {
    assert_eq!(
        bridge(&v("0.1.5"), &v("0.1.8"), &vs(&["0.1.6", "0.1.7"])),
        Some(vs(&["0.1.6", "0.1.7"]))
    );
}

#[test]
fn an_unclosable_gap_is_none_not_empty() {
    assert_eq!(bridge(&v("0.1.5"), &v("0.1.8"), &vs(&["0.1.6"])), None);
    assert!(bridge(&v("0.1.5"), &v("0.1.8"), &vs(&["0.1.6", "0.1.7"])).is_some());
}

#[test]
fn a_minor_gap_closes_by_a_minor_step() {
    assert_eq!(
        bridge(&v("0.1.10"), &v("0.3.0"), &vs(&["0.2.0"])),
        Some(vs(&["0.2.0"]))
    );
}

#[test]
fn prereleases_cannot_close_a_gap() {
    assert_eq!(
        bridge(&v("0.1.5"), &v("0.1.7"), &vs(&["0.1.6-alpha.1"])),
        None
    );
}

#[test]
fn the_first_descent_is_where_the_line_turns_back() {
    assert_eq!(first_descent(&vs(&["0.1.0", "0.2.0", "0.1.5"])), Some(2));
    assert_eq!(first_descent(&vs(&["0.1.0", "0.1.0", "0.2.0"])), None);
    assert_eq!(first_descent(&[]), None);
}

/// Everything the registry holds, yanked ones included, as the sequence
/// rows read it.
fn taken(live: &[&str], yanked: &[&str]) -> Vec<Version> {
    let mut out = vs(live);
    out.extend(vs(yanked));
    out
}

#[test]
fn a_yanked_version_is_not_a_hole() {
    let t = taken(&["0.3.0-alpha.2", "0.3.2"], &["0.3.0-alpha.2"]);
    assert_eq!(sandwiched(&vs(&["0.3.0-alpha.2", "0.3.2"]), &t), vec![]);
}

#[test]
fn a_genuinely_unpublished_version_below_the_top_is_a_hole() {
    let found = sandwiched(&vs(&["0.1.0", "0.3.2"]), &taken(&["0.3.2"], &[]));
    assert_eq!(found, vs(&["0.1.0"]));
}

#[test]
fn a_tag_above_everything_published_is_not_a_hole() {
    assert_eq!(
        sandwiched(&vs(&["0.3.2", "0.4.0"]), &taken(&["0.3.2"], &[])),
        vec![]
    );
}

#[test]
fn nothing_published_yet_means_no_holes() {
    assert_eq!(sandwiched(&vs(&["0.1.0"]), &taken(&[], &[])), vec![]);
}

#[test]
fn a_yanked_top_still_anchors_the_sequence() {
    let found = sandwiched(&vs(&["0.1.0", "0.3.2"]), &taken(&[], &["0.3.2"]));
    assert_eq!(found.len(), 1);
}
