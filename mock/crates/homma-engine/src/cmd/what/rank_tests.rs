//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::doc::{Doc, Kind, Verb};
use super::rank::{rank, related, required};
use super::text::pairs;

fn terms(q: &str) -> Vec<String> {
    pairs(q).into_iter().map(|(_, s)| s).collect()
}

fn doc(kind: Kind, name: &str, summary: &str, tags: &[&str]) -> Doc {
    let mut d = Doc::new(kind, name, name, summary);
    d.tags = tags.iter().map(|t| t.to_string()).collect();
    d
}

fn names(docs: &[Doc], q: &str) -> Vec<String> {
    rank(docs, &terms(q))
        .iter()
        .map(|h| docs[h.doc].name.clone())
        .collect()
}

#[test]
fn a_query_must_match_both_of_two_words_two_of_up_to_six_and_a_third_of_a_longer_one() {
    let got: Vec<usize> = (1 ..= 7).map(required).collect();
    assert_eq!(got, [1, 2, 2, 2, 2, 2, 3]);
}

#[test]
fn a_thing_matching_none_of_the_terms_is_not_an_answer() {
    let docs = vec![doc(Kind::Tool, "disk-trim", "give the disk back", &["hygiene"])];
    assert!(names(&docs, "bake sourdough loaf").is_empty());
}

#[test]
fn a_thing_matching_too_few_terms_is_not_an_answer() {
    let docs = vec![doc(Kind::Tool, "disk-trim", "give the disk back", &[])];
    assert!(
        names(&docs, "translate disk quarterly report").is_empty(),
        "one of three is under half"
    );
    assert_eq!(names(&docs, "trim disk"), ["disk-trim"]);
}

#[test]
fn a_name_outweighs_a_summary() {
    let docs = vec![
        doc(Kind::Script, "a-tidy", "land things somewhere", &[]),
        doc(Kind::Script, "land-it", "does something", &[]),
    ];
    assert_eq!(names(&docs, "land"), ["land-it", "a-tidy"]);
}

#[test]
fn a_word_every_thing_carries_does_not_decide() {
    // Every doc matches two terms; the padding matches the two that nearly everything carries, `y` the
    // two that only it carries. Without the inverse weighting the scores tie and the name order puts the
    // padding first.
    let mut docs: Vec<Doc> = (0 .. 12)
        .map(|i| {
            doc(
                Kind::Script,
                &format!("a{i}"),
                "works on a common branch",
                &[],
            )
        })
        .collect();
    docs.push(doc(Kind::Script, "zz-rare", "ends a lost stray", &[]));
    assert_eq!(names(&docs, "common branch lost stray")[0], "zz-rare");
}

#[test]
fn a_tie_breaks_on_kind_and_then_name() {
    let docs = vec![
        doc(Kind::Script, "b-widget", "does widget", &[]),
        doc(Kind::Tool, "z-widget", "does widget", &[]),
        doc(Kind::Script, "a-widget", "does widget", &[]),
    ];
    assert_eq!(names(&docs, "widget"), ["z-widget", "a-widget", "b-widget"]);
}

#[test]
fn a_skill_ranks_below_a_tool_that_is_a_little_weaker() {
    // The skill matches in a tag (4.0), the tool in a verb name (3.5): flat, the skill wins; with the
    // skill at four fifths it is 3.2 and the tool wins.
    let mut tool = doc(Kind::Tool, "tool", "unrelated words", &[]);
    tool.verbs = vec![Verb {
        name:    "trim".into(),
        summary: String::new(),
        command: "tool trim".into(),
    }];
    let docs = vec![doc(Kind::Skill, "aaa-skill", "other words", &["trim"]), tool];
    assert_eq!(names(&docs, "trim"), ["tool", "aaa-skill"]);
}

#[test]
fn a_rule_ranks_below_a_tool_that_is_a_little_weaker() {
    // The rule matches in a tag (4.0), the tool in a summary (2.5): flat, the rule wins; at three fifths
    // it is 2.4 and the tool wins.
    let docs = vec![
        doc(Kind::Rule, "aaa-rule", "other words", &["trim"]),
        doc(Kind::Tool, "tool", "will trim it", &[]),
    ];
    assert_eq!(names(&docs, "trim"), ["tool", "aaa-rule"]);
}

#[test]
fn an_exact_match_outranks_a_misspelling_in_the_same_field() {
    let docs = vec![doc(Kind::Tool, "a-dsik", "x", &[]), doc(Kind::Tool, "z-disk", "x", &[])];
    assert_eq!(names(&docs, "disk")[0], "z-disk");
}

#[test]
fn a_thing_matching_too_few_of_a_long_query_is_not_an_answer() {
    // `one` matches a single term but in its name, so it scores near `two`, which matches two in its
    // summary. Only the coverage rule drops it; the cut would keep it.
    let many = terms("alpha beta gamma delta epsilon zeta");
    let mut docs = vec![
        doc(Kind::Tool, "two", "alpha beta", &[]),
        doc(Kind::Tool, "zeta", "nothing", &[]),
    ];
    docs.extend((0 .. 30).map(|i| doc(Kind::Tool, &format!("pad{i}"), "nothing", &[])));
    let r = names(&docs, "alpha beta gamma delta epsilon zeta");
    assert_eq!(many.len(), 6);
    assert_eq!(
        r,
        ["two"],
        "a single term of six is not an answer, however it scores"
    );
}

#[test]
fn the_verb_that_matched_best_is_named() {
    let mut d = doc(
        Kind::Script,
        "sessions",
        "which helper sessions are what",
        &[],
    );
    d.verbs = vec![
        Verb {
            name:    "blocked".into(),
            summary: "ids of helpers stopped on a refusal".into(),
            command: "sessions blocked".into(),
        },
        Verb {
            name:    "finished".into(),
            summary: "ids of idle helpers whose channel says done".into(),
            command: "sessions finished".into(),
        },
    ];
    let docs = vec![d];
    let r = rank(&docs, &terms("which helpers are done"));
    assert_eq!(docs[0].verbs[r[0].verb.expect("a verb")].name, "finished");
}

#[test]
fn the_best_way_a_word_matches_is_the_one_kept() {
    // The body is read before the place, so a tie between them would keep the body.
    let mut d = doc(Kind::Tool, "x", "unrelated", &[]);
    d.body = "worktree".to_string();
    d.place = vec!["worktree".to_string()];
    let docs = vec![d];
    let r = rank(&docs, &terms("worktree"));
    assert_eq!(r[0].matches.len(), 1);
    assert_eq!(
        r[0].matches[0].field,
        super::rank::Field::Place,
        "the same stem in the place beats the same stem in the body"
    );
}

#[test]
fn the_share_of_the_query_a_thing_covers_decides_between_equal_sums() {
    // Both sum to twelve fields-worth: `alpha-beta` is two terms in its name, `zzz` three in its tags.
    // The third term is carried by `lone` so every term is as rare as the others.
    let docs = vec![
        doc(Kind::Tool, "alpha-beta", "x", &[]),
        doc(Kind::Tool, "zzz", "x", &["alpha", "beta", "gamma"]),
        doc(Kind::Tool, "lone", "gamma", &[]),
    ];
    assert_eq!(names(&docs, "alpha beta gamma")[0], "zzz");
}

#[test]
fn related_things_are_the_ones_nearest_in_their_words() {
    let mut docs = vec![
        doc(
            Kind::Tool,
            "disk-trim",
            "give the disk back by clearing stale build output",
            &["hygiene", "disk"],
        ),
        doc(
            Kind::Tool,
            "build-sweep",
            "clear stale build output to give disk back",
            &["hygiene"],
        ),
        doc(Kind::Tool, "mail-box", "post mail to a channel", &["mail"]),
    ];
    docs.extend((0 .. 20).map(|i| {
        doc(Kind::Tool, &format!("t{i}"), "something else entirely", &[
            "other",
        ])
    }));
    let ranked = rank(&docs, &terms("disk"));
    assert_eq!(docs[ranked[0].doc].name, "disk-trim");
    let rel = related(&docs, &ranked[.. 1], 1, 3);
    let got: Vec<&str> = rel[0].iter().map(|&i| docs[i].name.as_str()).collect();
    assert_eq!(
        got,
        ["build-sweep"],
        "a thing that shares nothing is not named"
    );
}

#[test]
fn related_never_names_an_answer() {
    let docs = vec![
        doc(Kind::Tool, "disk-trim", "give the disk back", &["hygiene"]),
        doc(Kind::Tool, "disk-sweep", "give the disk back", &["hygiene"]),
    ];
    let ranked = rank(&docs, &terms("disk"));
    assert_eq!(ranked.len(), 2);
    assert!(related(&docs, &ranked, 2, 3).iter().all(Vec::is_empty));
}

#[test]
fn a_tool_is_not_related_to_a_rule_by_a_shared_word() {
    let docs = vec![
        doc(Kind::Tool, "disk-trim", "give the disk back", &["hygiene"]),
        doc(Kind::Tool, "pad", "other", &[]),
        doc(Kind::Rule, "disk-trim-rule", "give the disk back", &[
            "hygiene",
        ]),
    ];
    let ranked = rank(&docs, &terms("trim"));
    let rel = related(&docs, &ranked[.. 1], 1, 3);
    assert!(rel[0].is_empty(), "{rel:?}");
}

#[test]
fn a_thing_under_a_fifth_of_the_best_is_dropped_by_the_cut_and_one_above_it_is_kept() {
    // All three match two of the three terms or more, so the coverage rule keeps each; `weak` finds
    // them only in its body, where they are worth little, so only the cut drops it.
    let mut weak = doc(Kind::Tool, "weak", "nothing", &[]);
    weak.body = "alpha beta".to_string();
    let mut docs = vec![
        doc(
            Kind::Tool,
            "alpha-beta-gamma",
            "alpha beta gamma together",
            &["alpha", "beta", "gamma"],
        ),
        weak,
        doc(Kind::Tool, "middling", "alpha beta gamma here", &[]),
    ];
    docs.extend((0 .. 30).map(|i| doc(Kind::Tool, &format!("pad{i}"), "nothing", &[])));
    let kept = names(&docs, "alpha beta gamma");
    assert_eq!(kept[0], "alpha-beta-gamma");
    assert!(kept.contains(&"middling".to_string()), "{kept:?}");
    assert!(!kept.contains(&"weak".to_string()), "{kept:?}");
}
