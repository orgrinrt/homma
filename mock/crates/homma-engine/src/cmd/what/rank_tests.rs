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
fn a_query_must_match_half_its_terms_and_a_third_of_a_long_one() {
    let got: Vec<usize> = (1 ..= 7).map(required).collect();
    assert_eq!(got, [1, 1, 2, 2, 2, 2, 3]);
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
        doc(Kind::Script, "tidy", "land things somewhere", &[]),
        doc(Kind::Script, "land-it", "does something", &[]),
    ];
    assert_eq!(names(&docs, "land"), ["land-it", "tidy"]);
}

#[test]
fn a_word_every_thing_carries_does_not_decide() {
    let mut docs: Vec<Doc> = (0 .. 12)
        .map(|i| doc(Kind::Script, &format!("s{i}"), "works on a branch", &[]))
        .collect();
    docs.push(doc(
        Kind::Script,
        "stray-runs",
        "ends a lost run on a branch",
        &[],
    ));
    assert_eq!(names(&docs, "lost branch")[0], "stray-runs");
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
fn a_rule_ranks_below_a_tool_that_says_the_same() {
    let docs = vec![
        doc(Kind::Rule, "trim-the-disk", "trim the disk", &[]),
        doc(Kind::Tool, "disk-trim", "trim the disk", &[]),
    ];
    assert_eq!(names(&docs, "trim disk")[0], "disk-trim");
}

#[test]
fn a_thing_far_behind_the_best_is_dropped() {
    let mut docs = vec![doc(Kind::Tool, "land-branch", "land a finished branch", &[
        "land",
    ])];
    docs.push(doc(Kind::Script, "far", &"x ".repeat(3), &["land"]));
    docs.extend((0 .. 30).map(|i| doc(Kind::Script, &format!("pad{i}"), "nothing", &[])));
    let r = rank(&docs, &terms("land branch"));
    assert!(r.iter().all(|h| h.score >= r[0].score * 0.2));
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
    let docs = vec![doc(Kind::Tool, "x", "worktrees of the clone", &["worktree"])];
    let r = rank(&docs, &terms("worktree"));
    assert_eq!(r[0].matches.len(), 1);
    assert_eq!(
        r[0].matches[0].field,
        super::rank::Field::Tag,
        "the same stem in a tag beats the same stem in a summary"
    );
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
