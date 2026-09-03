//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `---` block every authored document here opens with.
//!
//! The rule and skill readers both sit on this, so what is tested is the split
//! itself: which keys it takes, what it refuses, and where the body starts.

use homma_api::frontmatter::{self, FrontmatterError};

const KNOWN: &[&str] = &["topics", "fires", "kind", "paths"];

fn split(src: &str) -> Result<frontmatter::Block, FrontmatterError> {
    frontmatter::split(src, KNOWN)
}

#[test]
fn the_body_starts_after_the_closing_fence() {
    let block = split("---\nkind: reflex\n---\n\n# Title\n\nProse.\n").unwrap();
    assert_eq!(block.body, "# Title\n\nProse.");
}

#[test]
fn a_fence_inside_the_body_is_not_the_closing_one() {
    // A horizontal rule in prose is spelled the same as the fence. The walk
    // stops at the first one after the opening, so a later one is body text.
    let block = split("---\nkind: reflex\n---\n\nBefore.\n\n---\n\nAfter.\n").unwrap();
    assert!(block.body.contains("Before."), "{}", block.body);
    assert!(block.body.contains("After."), "{}", block.body);
}

#[test]
fn a_file_that_does_not_open_with_a_fence_is_refused() {
    let err = split("# Title\n\n---\nkind: reflex\n---\n").unwrap_err();
    assert_eq!(err, FrontmatterError::NoFrontmatter);
}

#[test]
fn a_block_that_never_closes_is_refused() {
    let err = split("---\nkind: reflex\n\n# Title\n").unwrap_err();
    assert_eq!(err, FrontmatterError::Unterminated);
}

#[test]
fn a_line_that_is_not_a_key_value_is_refused_with_its_number() {
    let err = split("---\nkind: reflex\njust some words\n---\n").unwrap_err();
    match err {
        FrontmatterError::NotAKeyValue {
            line,
            challenge,
        } => {
            assert_eq!(line, 3);
            assert_eq!(challenge, "just some words");
        },
        other => panic!("wrong refusal: {other}"),
    }
}

#[test]
fn a_repeated_key_is_refused_rather_than_silently_keeping_one() {
    let err = split("---\nkind: reflex\nkind: discipline\n---\n").unwrap_err();
    assert!(
        matches!(err, FrontmatterError::DuplicateKey {
            line: 3,
            ..
        }),
        "{err}"
    );
}

#[test]
fn a_key_outside_the_known_set_is_refused_rather_than_dropped() {
    let err = split("---\nkind: reflex\ntopcis: [a]\n---\n").unwrap_err();
    match err {
        FrontmatterError::UnknownKey {
            key,
            ..
        } => assert_eq!(key, "topcis"),
        other => panic!("a typo must be named, not dropped: {other}"),
    }
}

#[test]
fn a_blank_line_or_a_comment_inside_the_block_carries_nothing() {
    let block = split("---\n\n# a note\nkind: reflex\n\n---\n\nBody.\n").unwrap();
    assert_eq!(block.fields.len(), 1);
    assert_eq!(block.body, "Body.");
}

#[test]
fn an_absent_optional_list_is_empty_and_an_absent_required_one_is_missing() {
    // The distinction the shared helper lost once: reporting a required list as
    // empty sends an author to look at a line they never wrote.
    let block = split("---\nkind: reflex\n---\n").unwrap();
    assert_eq!(block.list("paths").unwrap(), Vec::<String>::new());
    assert_eq!(
        block.required_list("topics").unwrap_err(),
        FrontmatterError::Missing {
            key: "topics",
        }
    );
}

#[test]
fn only_the_inline_list_form_is_taken() {
    let block = split("---\ntopics: [a, b]\n---\n").unwrap();
    assert_eq!(block.list("topics").unwrap(), vec![
        "a".to_string(),
        "b".to_string()
    ]);

    // The block form YAML also allows would parse as a scalar and take the
    // rest of the list as unknown keys, so it is refused where it is read.
    let block = split("---\ntopics: a\n---\n").unwrap();
    assert!(matches!(
        block.list("topics"),
        Err(FrontmatterError::BadValue { .. })
    ));
}

#[test]
fn quotes_come_off_a_scalar_and_off_every_list_member() {
    let block = split("---\nfires: \"when a thing happens\"\ntopics: ['a', \"b\"]\n---\n").unwrap();
    assert_eq!(block.scalar("fires").unwrap(), "when a thing happens");
    assert_eq!(block.list("topics").unwrap(), vec![
        "a".to_string(),
        "b".to_string()
    ]);
}

#[test]
fn a_scalar_holding_a_colon_keeps_everything_after_the_first_one() {
    // The split is on the first colon, so a value may contain more.
    let block = split("---\nfires: \"before this: and that\"\n---\n").unwrap();
    assert_eq!(block.scalar("fires").unwrap(), "before this: and that");
}

#[test]
fn an_empty_required_scalar_is_refused() {
    let block = split("---\nfires: \"\"\n---\n").unwrap();
    assert!(matches!(
        block.scalar("fires"),
        Err(FrontmatterError::BadValue { .. })
    ));
}
