//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The acceptance set: queries run against the planted workspace of [`super::fixture`], each
//! checking a mechanism of the matcher. Which of a real workspace's tools comes first for its own
//! queries is that workspace's to test.

use super::doc::Kind;
use super::fixture::workspace;
use super::{Answer, answer, gather};

/// Everything the planted workspace holds, read once for the whole set. The mockspace listing is
/// left out: nothing planted is a mockspace repository.
fn everything() -> &'static (Vec<super::doc::Doc>, Vec<super::doc::Gap>) {
    static ALL: std::sync::OnceLock<(Vec<super::doc::Doc>, Vec<super::doc::Gap>)> =
        std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let g = gather(
            workspace().path(),
            &[],
            true,
            &homma_core::scope::Scope::everything(),
        );
        (g.docs, g.gaps)
    })
}

fn ask(q: &str) -> Answer {
    let (docs, gaps) = everything();
    answer(docs, gaps.clone(), q, 5)
}

fn first(q: &str) -> (Kind, String, String) {
    let a = ask(q);
    let h = a
        .hits
        .first()
        .unwrap_or_else(|| panic!("`{q}` found nothing"));
    (h.kind, h.name.clone(), h.command.clone())
}

fn must_come_first(q: &str, kind: Kind, name: &str) {
    let (k, n, _) = first(q);
    assert_eq!(
        (k, n.as_str()),
        (kind, name),
        "`{q}` came first with the wrong thing"
    );
}

#[test]
fn a_script_is_found_by_its_summary() {
    must_come_first("water the plants", Kind::Script, "water-plants");
}

#[test]
fn a_scripts_verb_that_matched_is_its_command() {
    must_come_first("which crops are ripe", Kind::Script, "harvest");
    assert_eq!(
        first("which crops are ripe").2,
        ".shared/scripts/chores/harvest ripe"
    );
}

#[test]
fn a_tools_verb_that_matched_is_its_command() {
    must_come_first("catch a pest", Kind::Tool, "pest-watch");
    assert_eq!(first("catch a pest").2, "homma pest-watch catch");
}

#[test]
fn a_misspelt_long_word_is_found_by_its_edit() {
    must_come_first("wtaer the palnts", Kind::Script, "water-plants");
}

#[test]
fn a_misspelt_query_finds_the_same_tool() {
    must_come_first("cacth a pset", Kind::Tool, "pest-watch");
}

/// Failing control: the library is planted executable with a header, so only its `Sourced` line
/// keeps it out.
#[test]
fn a_sourced_library_is_never_an_answer() {
    let (docs, _) = everything();
    assert!(
        !docs.iter().any(|d| d.command.contains("bed-names")),
        "a script whose header says it is sourced was offered as a command"
    );
}

#[test]
fn what_a_kind_filter_leaves_out_is_said_in_the_footer() {
    let g = gather(
        workspace().path(),
        &[Kind::Rule],
        true,
        &homma_core::scope::Scope::everything(),
    );
    let a = answer(&g.docs, g.gaps, "water the plants", 5);
    assert!(
        a.gaps
            .iter()
            .any(|g| g.what.contains("left out by --kind") && g.what.contains("script")),
        "{:?}",
        a.gaps
    );
}

#[test]
fn the_filler_around_a_query_changes_nothing() {
    let plain = ask("water the plants");
    let said = ask("homma what to use to water the plants");
    let name = |a: &Answer| a.hits.first().map(|h| h.name.clone());
    assert_eq!(name(&plain), name(&said));
    assert!(name(&plain).is_some());
}

#[test]
fn a_query_about_nothing_the_tooling_does_returns_nothing() {
    for q in [
        "bake a sourdough loaf",
        "translate this document into french",
        "book a flight to helsinki",
        "bake bread",
        "book flight",
    ] {
        let a = ask(q);
        assert!(
            a.hits.is_empty(),
            "`{q}` answered with {:?}",
            a.hits.iter().map(|h| &h.name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_query_of_only_filler_returns_nothing_and_says_why() {
    let a = ask("what is the way to do it");
    assert!(a.hits.is_empty() && a.terms.is_empty());
    let mut out = Vec::new();
    crate::output::HumanRender::render_human(&a, &mut out).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("filler"));
}

#[test]
fn every_answer_says_where_each_word_was_found() {
    let a = ask("water the plants");
    let m = &a.hits[0].matched;
    assert!(m.iter().any(|m| m.word == "water" && m.how == "="), "{m:?}");
    assert!(
        m.iter().any(|m| m.word == "plants" && m.how == "="),
        "{m:?}"
    );
}

#[test]
fn the_json_carries_the_query_the_terms_the_hits_and_the_gaps() {
    let v = serde_json::to_value(ask("clear the beds of weeds")).unwrap();
    for key in ["query", "terms", "read", "hits", "gaps"] {
        assert!(v.get(key).is_some(), "no `{key}` in {v}");
    }
    let h = &v["hits"][0];
    for key in ["kind", "name", "command", "summary", "score", "matched", "related"] {
        assert!(h.get(key).is_some(), "no `{key}` in {h}");
    }
    assert_eq!(h["kind"], "tool");
}

/// Failing control: the planted tools have verbs and neither a verb of its own name, so a bare
/// `homma plot-weed` would be listed.
#[test]
fn a_tool_with_verbs_and_no_default_one_never_prints_a_command_the_dispatcher_refuses() {
    let (docs, _) = everything();
    let tools: Vec<&str> = docs
        .iter()
        .filter(|d| d.kind == Kind::Tool && !d.verbs.is_empty())
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(tools.len(), 3, "{tools:?}");
    let bare: Vec<&str> = docs
        .iter()
        .filter(|d| d.kind == Kind::Tool && !d.verbs.is_empty())
        .filter(|d| !d.verbs.iter().any(|v| v.name == d.name))
        .filter(|d| !d.command.contains('<'))
        .map(|d| d.command.as_str())
        .collect();
    assert!(bare.is_empty(), "bare commands that want a verb: {bare:?}");
    let a = ask("garden");
    assert!(
        a.hits.iter().any(|h| h.kind == Kind::Tool),
        "the query finds the tools"
    );
    assert!(
        a.hits.iter().all(|h| {
            h.kind != Kind::Tool || h.command.contains('<') || h.command.matches(' ').count() >= 2
        }),
        "a tool hit printed bare: {:?}",
        a.hits.iter().map(|h| &h.command).collect::<Vec<_>>()
    );
}

#[test]
fn the_best_answers_name_what_sits_beside_them() {
    let a = ask("the last weeding");
    assert_eq!(a.hits[0].name, "plot-weed");
    let related: Vec<&str> = a.hits[0].related.iter().map(|r| r.name.as_str()).collect();
    assert!(related.contains(&"plot-mulch"), "{related:?}");
}
