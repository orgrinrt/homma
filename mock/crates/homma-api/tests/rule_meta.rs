//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading a rule's meta off its full template.
//!
//! The reader is deliberately narrow, so most of what is worth testing is what
//! it refuses. A reader that accepted everything would need none of these and
//! would silently drop the fields it could not render.

use homma_api::rule::{self, MetaError, RuleKind};

/// A well-formed template, which every refusal below is a mutation of.
fn sound() -> String {
    [
        "---",
        "topics: [writing, prose, readme]",
        "fires: \"before writing any prose a human reads\"",
        "kind: reflex",
        "---",
        "",
        "The body, which is the elaboration.",
    ]
    .join("\n")
}

#[test]
fn reads_every_field_and_the_body() {
    let p = rule::parse(&sound()).expect("the sound fixture must parse");
    assert_eq!(p.meta.topics, vec!["writing", "prose", "readme"]);
    assert_eq!(p.meta.fires, "before writing any prose a human reads");
    assert_eq!(p.meta.kind, RuleKind::Reflex);
    assert!(
        p.meta.paths.is_empty(),
        "paths is absent, so it is empty rather than defaulted to a glob"
    );
    assert_eq!(p.body, "The body, which is the elaboration.");
}

#[test]
fn the_body_excludes_the_frontmatter() {
    // The failure this catches is a body that starts one line early and carries
    // the closing fence into the rendered card, which reads as a stray rule.
    let p = rule::parse(&sound()).unwrap();
    assert!(
        !p.body.contains("---"),
        "body carried the fence: {:?}",
        p.body
    );
    assert!(
        !p.body.contains("topics"),
        "body carried a field: {:?}",
        p.body
    );
}

#[test]
fn a_file_with_no_frontmatter_is_refused() {
    let err = rule::parse("# Just a heading\n\nAnd prose.").unwrap_err();
    assert_eq!(err, MetaError::NoFrontmatter);
}

#[test]
fn an_unterminated_block_is_refused() {
    // Without this the reader would treat the whole file as frontmatter and
    // report a missing key, sending the author to add one that is already there.
    let src = "---\ntopics: [a]\nfires: \"x\"\nkind: reflex\n";
    assert_eq!(rule::parse(src).unwrap_err(), MetaError::Unterminated);
}

#[test]
fn a_line_that_is_not_key_value_is_refused_and_named() {
    let src = "---\ntopics: [a]\nthis is not a field\nfires: \"x\"\nkind: reflex\n---\n";
    match rule::parse(src).unwrap_err() {
        MetaError::NotAKeyValue {
            line,
            ..
        } => assert_eq!(line, 3),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn a_duplicated_key_is_refused() {
    // Two values for one field keeps one of them and drops the other with no
    // word about which, so it is refused rather than resolved.
    let src = "---\ntopics: [a]\ntopics: [b]\nfires: \"x\"\nkind: reflex\n---\n";
    match rule::parse(src).unwrap_err() {
        MetaError::DuplicateKey {
            key,
            ..
        } => assert_eq!(key, "topics"),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn an_unknown_key_is_refused_rather_than_ignored() {
    // Nearly always a typo for a field that exists. Ignored, the author's
    // intent is dropped and the generated card looks fine.
    let src = "---\ntopics: [a]\nfires: \"x\"\nkind: reflex\ntopic: [b]\n---\n";
    match rule::parse(src).unwrap_err() {
        MetaError::UnknownKey {
            key,
            ..
        } => assert_eq!(key, "topic"),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn each_required_key_is_required() {
    // One case per key rather than one for the set, so dropping the check on a
    // single field cannot pass by way of another still being enforced.
    for (drop, expect) in [
        ("topics: [writing, prose, readme]", "topics"),
        ("fires: \"before writing any prose a human reads\"", "fires"),
        ("kind: reflex", "kind"),
    ] {
        let src = sound().replace(drop, "");
        match rule::parse(&src).unwrap_err() {
            MetaError::Missing {
                key,
            } => assert_eq!(key, expect, "wrong key reported missing"),
            other => panic!("dropping `{drop}` gave {other:?}"),
        }
    }
}

#[test]
fn an_empty_topic_list_is_refused() {
    // It parses as a list and would leave a rule nothing can ever discover,
    // which is the one thing the field exists for.
    let src = sound().replace("topics: [writing, prose, readme]", "topics: []");
    match rule::parse(&src).unwrap_err() {
        MetaError::BadValue {
            key,
            ..
        } => assert_eq!(key, "topics"),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn an_empty_trigger_is_refused() {
    let src = sound().replace(
        "fires: \"before writing any prose a human reads\"",
        "fires: \"\"",
    );
    match rule::parse(&src).unwrap_err() {
        MetaError::BadValue {
            key,
            ..
        } => assert_eq!(key, "fires"),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn a_kind_outside_the_two_is_refused() {
    let src = sound().replace("kind: reflex", "kind: guideline");
    match rule::parse(&src).unwrap_err() {
        MetaError::BadValue {
            key,
            ..
        } => assert_eq!(key, "kind"),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn the_block_list_spelling_is_refused_rather_than_supported() {
    // YAML also writes a list as indented dashes. Supporting one spelling
    // quietly is how a corpus ends up carrying both, so the other is named.
    let src = "---\ntopics:\n  - writing\nfires: \"x\"\nkind: reflex\n---\n";
    let err = rule::parse(src).unwrap_err();
    assert!(
        matches!(
            err,
            MetaError::BadValue { .. } | MetaError::NotAKeyValue { .. }
        ),
        "block lists must be refused, got {err:?}"
    );
}

#[test]
fn paths_is_optional_and_read_when_present() {
    let src = sound().replace(
        "kind: reflex",
        "kind: reflex\npaths: [\"**/*.rs\", \"**/*.md\"]",
    );
    let p = rule::parse(&src).unwrap();
    assert_eq!(p.meta.paths, vec!["**/*.rs", "**/*.md"]);
}

#[test]
fn blank_lines_and_comments_inside_the_block_carry_nothing() {
    let src =
        "---\n# what this rule is about\ntopics: [a]\n\nfires: \"x\"\nkind: reflex\n---\nbody";
    let p = rule::parse(src).expect("comments and blanks are allowed");
    assert_eq!(p.meta.topics, vec!["a"]);
}

#[test]
fn quotes_are_optional_on_a_scalar() {
    let a = rule::parse(&sound()).unwrap();
    let b = rule::parse(&sound().replace(
        "fires: \"before writing any prose a human reads\"",
        "fires: before writing any prose a human reads",
    ))
    .unwrap();
    assert_eq!(a.meta.fires, b.meta.fires);
}

#[test]
fn scoring_counts_the_query_terms_a_rule_answers() {
    let p = rule::parse(&sound()).unwrap();
    let q = rule::query_terms("writing, readme, public");
    // Two of the three: `writing` and `readme` are topics, `public` is not.
    assert_eq!(rule::score(&p.meta, &q), 2);
}

#[test]
fn a_rule_answering_nothing_scores_zero() {
    // The control. Without it a scorer returning a constant would pass the
    // case above and every rule would come back for every query.
    let p = rule::parse(&sound()).unwrap();
    assert_eq!(
        rule::score(&p.meta, &rule::query_terms("vulkan, shaders")),
        0
    );
}

#[test]
fn scoring_matches_a_term_inside_a_topic_and_the_other_way() {
    // Somebody asking about `writing` should reach a rule tagged
    // `writing-style`, and one asking about `readme-format` should reach
    // `readme`. Equality alone answers neither.
    let src = sound().replace(
        "topics: [writing, prose, readme]",
        "topics: [writing-style]",
    );
    let p = rule::parse(&src).unwrap();
    assert_eq!(rule::score(&p.meta, &rule::query_terms("writing")), 1);
    assert_eq!(
        rule::score(&p.meta, &rule::query_terms("writing-style-guide")),
        1
    );
}

#[test]
fn scoring_ignores_case() {
    let p = rule::parse(&sound()).unwrap();
    assert_eq!(rule::score(&p.meta, &rule::query_terms("WRITING")), 1);
}

#[test]
fn a_query_splits_on_commas_or_spaces_or_both() {
    let expect = vec!["writing".to_string(), "readme".to_string(), "public".to_string()];
    assert_eq!(rule::query_terms("writing, readme, public"), expect);
    assert_eq!(rule::query_terms("writing readme public"), expect);
    assert_eq!(rule::query_terms("  writing ,readme,  public "), expect);
}

// -------------------------------------------------------------------------
// the card is a prefix of the full rule
// -------------------------------------------------------------------------

/// A rule with both halves.
fn split() -> String {
    [
        "---",
        "topics: [a]",
        "fires: \"x\"",
        "kind: reflex",
        "---",
        "",
        "# Title",
        "",
        "The card.",
        "",
        "<!-- elaboration -->",
        "",
        "The depth.",
    ]
    .join("\n")
}

#[test]
fn the_card_is_everything_before_the_marker() {
    let p = rule::parse(&split()).unwrap();
    assert!(p.card().contains("The card."));
    assert!(p.card().contains("# Title"));
    assert!(
        !p.card().contains("The depth."),
        "the card ran past the marker: {:?}",
        p.card()
    );
    assert!(
        !p.card().contains(rule::ELABORATION_MARKER),
        "the marker leaked into the card: {:?}",
        p.card()
    );
}

#[test]
fn the_elaboration_is_everything_after_it() {
    let p = rule::parse(&split()).unwrap();
    assert!(p.elaboration().contains("The depth."));
    assert!(
        !p.elaboration().contains("The card."),
        "the elaboration ran back before the marker: {:?}",
        p.elaboration()
    );
    assert!(p.has_elaboration());
}

#[test]
fn a_rule_with_no_marker_is_all_card() {
    // The shape of a rule whose whole statement fits. Treating a missing marker
    // as a defect would refuse most of the corpus for being short.
    let p = rule::parse(&sound()).unwrap();
    assert_eq!(p.card(), p.body.trim_end());
    assert_eq!(p.elaboration(), "");
    assert!(!p.has_elaboration());
}

#[test]
fn the_two_halves_do_not_overlap() {
    // The control for both cases above: a split that returned the whole body on
    // each side would satisfy every `contains` assertion written so far.
    let p = rule::parse(&split()).unwrap();
    assert_ne!(p.card(), p.elaboration());
    assert!(!p.card().is_empty());
    assert!(!p.elaboration().is_empty());
}
