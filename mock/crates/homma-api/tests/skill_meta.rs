//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a skill declares about itself.
//!
//! Two fields are the corpus's own and the rest are the host's. The reader is
//! narrow on purpose, so most of what is worth asserting is what it refuses.

use homma_api::skill::{self, SkillError};

fn manifest(extra: &str) -> String {
    format!(
        "---\nname: pr-review\ndescription: Use when reviewing a pull request.\n{extra}---\n\n# pr-review\n\nThe body.\n"
    )
}

#[test]
fn the_two_fields_the_corpus_owns_are_read() {
    let parsed = skill::parse(&manifest("")).unwrap();
    assert_eq!(parsed.meta.name, "pr-review");
    assert_eq!(
        parsed.meta.description,
        "Use when reviewing a pull request."
    );
    assert_eq!(parsed.body, "# pr-review\n\nThe body.");
}

#[test]
fn a_missing_name_is_refused() {
    let src = "---\ndescription: x\n---\n\n# t\n";
    assert_eq!(skill::parse(src).unwrap_err(), SkillError::Missing {
        key: "name",
    });
}

#[test]
fn a_missing_description_is_refused_because_the_listing_carries_it() {
    let src = "---\nname: pr-review\n---\n\n# t\n";
    assert_eq!(skill::parse(src).unwrap_err(), SkillError::Missing {
        key: "description",
    });
}

#[test]
fn an_empty_description_is_refused_and_says_why() {
    let src = "---\nname: pr-review\ndescription: \"\"\n---\n\n# t\n";
    match skill::parse(src).unwrap_err() {
        SkillError::BadValue {
            key,
            reason,
        } => {
            assert_eq!(key, "description");
            assert!(
                reason.contains("the listing carries this"),
                "the refusal says what is lost: {reason}"
            );
        },
        other => panic!("wrong refusal: {other}"),
    }
}

#[test]
fn the_hosts_own_fields_are_taken_and_kept() {
    // Taken rather than refused, so a skill legitimately using one is not
    // blocked; kept rather than dropped, so generation round-trips it.
    let parsed = skill::parse(&manifest("allowed-tools: Read, Grep\nmodel: sonnet\n")).unwrap();
    assert!(
        parsed
            .meta
            .extra
            .iter()
            .any(|(k, v)| k == "allowed-tools" && v == "Read, Grep")
    );
    assert!(
        parsed
            .meta
            .extra
            .iter()
            .any(|(k, v)| k == "model" && v == "sonnet")
    );
}

#[test]
fn a_field_outside_the_known_set_is_refused_by_name() {
    match skill::parse(&manifest("descriptoin: y\n")).unwrap_err() {
        SkillError::UnknownKey {
            key,
            ..
        } => assert_eq!(key, "descriptoin"),
        other => panic!("a typo must be named: {other}"),
    }
}

#[test]
fn a_declared_name_is_checked_against_the_directory_holding_it() {
    let parsed = skill::parse(&manifest("")).unwrap();
    assert!(skill::name_matches_dir(&parsed.meta, "pr-review"));
    assert!(
        !skill::name_matches_dir(&parsed.meta, "pr-reviews"),
        "a skill documented under one name and invoked under another is a defect"
    );
}

#[test]
fn a_description_may_carry_a_colon() {
    let parsed =
        skill::parse("---\nname: t\ndescription: Use when: reviewing.\n---\n\n# t\n").unwrap();
    assert_eq!(parsed.meta.description, "Use when: reviewing.");
}
