//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a brief picks, what it keeps of the answer, and what it does without a pass.

use std::cell::{Cell, RefCell};

use homma_api::SummaryConfig;

use super::brief::*;
use super::doc::{Doc, Kind};
use super::rank;
use super::vocab::Vocabulary;
use crate::cmd::ask::cache::Cache;
use crate::cmd::summary::OneShot;

/// A model that gives one answer and counts the calls.
struct Fake {
    answer: String,
    calls:  Cell<usize>,
    seen:   RefCell<String>,
}

impl Fake {
    fn new(answer: &str) -> Fake {
        Fake {
            answer: answer.to_string(),
            calls:  Cell::new(0),
            seen:   RefCell::new(String::new()),
        }
    }
}

impl OneShot for Fake {
    fn ask(&self, system: &str, content: &str) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        *self.seen.borrow_mut() = format!("{system}\n----\n{content}");
        Ok(self.answer.clone())
    }
}

fn on() -> SummaryConfig {
    SummaryConfig {
        enabled: true,
        ..SummaryConfig::default()
    }
}

fn rule(name: &str, fires: &str, text: &str) -> Doc {
    let mut d = Doc::new(Kind::Rule, name, &format!("homma rules show {name}"), fires);
    d.text = text.to_string();
    d.body = text.to_string();
    d
}

/// Two rules; the content is `=== rule tests ...` at line 1, its three lines, then `=== rule
/// prose ...` at line 5 and its two lines.
fn docs() -> Vec<Doc> {
    vec![
        rule(
            "tests",
            "Fires when writing tests.",
            "# Tests fail first\nEvery test is seen failing before it passes.\nA tautology is no test.",
        ),
        rule(
            "prose",
            "Fires when writing prose.",
            "# Plain prose\nNo em-dashes in any prose.",
        ),
    ]
}

fn made(fake: &Fake, docs: &[Doc]) -> Brief {
    brief(
        fake,
        &on(),
        None,
        docs,
        &[0, 1],
        "write a test for the parser",
    )
}

#[test]
fn the_content_opens_each_candidate_with_its_heading() {
    let fake = Fake::new("- line 1: rule tests: the work writes a test");
    made(&fake, &docs());
    let seen = fake.seen.borrow();
    assert!(
        seen.contains("1\t=== rule tests: homma rules show tests ==="),
        "{seen}"
    );
    assert!(
        seen.contains("5\t=== rule prose: homma rules show prose ==="),
        "{seen}"
    );
    assert!(
        seen.contains("write a test for the parser"),
        "the work is in the brief"
    );
}

#[test]
fn a_quote_its_source_does_not_hold_is_dropped_and_counted() {
    let fake = Fake::new(
        "- line 1: rule tests: the work writes a test\n- line 3: \"Every test is seen failing \
         before it passes.\"\n- line 4: \"A test may pass at once.\"",
    );
    let b = made(&fake, &docs());
    assert_eq!(b.things.len(), 1);
    assert_eq!(b.things[0].quotes, [
        "Every test is seen failing before it passes."
    ]);
    assert_eq!(b.dropped, 1);
}

#[test]
fn a_quote_is_checked_with_its_whitespace_folded_and_in_the_lines_it_names() {
    let fake = Fake::new(
        "- line 1: rule tests: it applies\n- lines 3-4: \"before it passes. A  tautology\"\n- line \
         2: \"A tautology is no test.\"",
    );
    let b = made(&fake, &docs());
    assert_eq!(b.things[0].quotes, ["before it passes. A  tautology"]);
    assert_eq!(
        b.dropped, 1,
        "a span on another line than the one named is not taken"
    );
}

#[test]
fn each_line_is_given_back_to_the_thing_its_place_belongs_to() {
    let fake = Fake::new(
        "- line 5: rule prose: the work writes a doc comment\n- line 7: \"No em-dashes in any \
         prose.\"\n- line 1: rule tests: the work writes a test",
    );
    let b = made(&fake, &docs());
    let names: Vec<&str> = b.things.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["prose", "tests"],
        "in the order the answer first names each"
    );
    assert_eq!(b.things[0].why, ["the work writes a doc comment"]);
    assert_eq!(b.things[0].quotes, ["No em-dashes in any prose."]);
    assert_eq!(b.things[0].command, "homma rules show prose");
    assert!(b.things[1].quotes.is_empty());
}

#[test]
fn a_line_with_no_place_or_one_past_the_end_is_dropped() {
    let fake = Fake::new(
        "- line 1: rule tests: it applies\n- line 40: \"Every test\"\n- somewhere: \"Every test\"",
    );
    let b = made(&fake, &docs());
    assert_eq!(b.things.len(), 1);
    assert_eq!(b.dropped, 2);
}

#[test]
fn a_thing_the_answer_does_not_name_is_not_printed() {
    let fake = Fake::new("- line 2: \"# Tests fail first\"");
    let b = made(&fake, &docs());
    assert_eq!(b.things.len(), 1);
    assert_eq!(
        b.things[0].name, "tests",
        "a quote names the thing that holds it"
    );
    assert!(b.things.iter().all(|t| t.name != "prose"));
}

#[test]
fn with_the_pass_off_the_brief_is_the_candidates_and_why_there_is_no_extract() {
    let fake = Fake::new("- line 1: rule tests: it applies");
    let b = brief(
        &fake,
        &SummaryConfig::default(),
        None,
        &docs(),
        &[0, 1],
        "a test",
    );
    assert_eq!(fake.calls.get(), 0);
    assert_eq!(b.things.len(), 2);
    assert_eq!(b.things[0].why, ["Fires when writing tests."]);
    assert!(
        b.note.as_deref().is_some_and(|n| n.contains("off")),
        "{:?}",
        b.note
    );
    // the pass on gives an extract and no note, which is the control
    let b = made(&fake, &docs());
    assert!(b.note.is_none(), "{:?}", b.note);
}

#[test]
fn a_failed_call_is_the_candidates_too() {
    struct Broken;
    impl OneShot for Broken {
        fn ask(&self, _: &str, _: &str) -> Result<String, String> {
            Err("no model here".to_string())
        }
    }
    let b = brief(&Broken, &on(), None, &docs(), &[0, 1], "a test");
    assert_eq!(b.things.len(), 2);
    assert!(
        b.note
            .as_deref()
            .is_some_and(|n| n.contains("no model here")),
        "{:?}",
        b.note
    );
}

#[test]
fn a_kept_brief_is_given_again_with_no_call_until_a_candidate_changes() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_path_buf());
    let fake = Fake::new("- line 1: rule tests: it applies");
    let mut d = docs();
    let first = brief(&fake, &on(), Some(&cache), &d, &[0, 1], "a test");
    assert!(!first.cached);
    let again = brief(&fake, &on(), Some(&cache), &d, &[0, 1], "a test");
    assert!(again.cached);
    assert_eq!(fake.calls.get(), 1);
    assert_eq!(again.things, first.things);
    // another work, or a rule whose text changed, is asked again
    let _ = brief(&fake, &on(), Some(&cache), &d, &[0, 1], "another test");
    assert_eq!(fake.calls.get(), 2);
    d[1].text.push_str("\nA new line.");
    let _ = brief(&fake, &on(), Some(&cache), &d, &[0, 1], "a test");
    assert_eq!(fake.calls.get(), 3);
}

#[test]
fn no_candidate_makes_no_call_and_an_empty_brief() {
    let fake = Fake::new("- line 1: rule tests: it applies");
    let b = brief(&fake, &on(), None, &docs(), &[], "a test");
    assert_eq!(fake.calls.get(), 0);
    assert!(b.things.is_empty());
}

#[test]
fn a_long_text_takes_a_thing_that_answers_two_of_its_words() {
    let d = docs();
    let work = "The parser reads a manifest, keeps every key it knows, refuses the rest by name, \
                and its tests cover each refusal and each default the manifest may leave out, \
                along with a tautology check over the suite";
    let terms: Vec<String> = super::text::pairs(work)
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    let v = Vocabulary::compiled();
    assert!(
        rank::rank(&d, &terms, v).is_empty(),
        "a query needs a third of its words"
    );
    assert_eq!(candidates(&d, work, LIMIT, v), [0]);
}

#[test]
fn a_work_over_the_budget_is_cut_and_says_so() {
    let fake = Fake::new("- line 1: rule tests: it applies");
    let work = "a line of the work\n".repeat(4000);
    let b = brief(&fake, &on(), None, &docs(), &[0, 1], &work);
    assert!(fake.seen.borrow().len() < work.len());
    assert!(b.cut.is_some_and(|n| n > 1), "{:?}", b.cut);
    // a short work is not cut, which is the control
    assert!(made(&fake, &docs()).cut.is_none());
}

#[test]
fn the_printed_brief_names_each_thing_its_command_why_and_quotes() {
    let fake = Fake::new(
        "- line 1: rule tests: the work writes a test\n- line 3: \"Every test is seen failing \
         before it passes.\"\n- line 4: \"made up\"",
    );
    let text = made(&fake, &docs()).render();
    assert!(
        text.contains("rule tests: homma rules show tests"),
        "{text}"
    );
    assert!(text.contains("  the work writes a test"), "{text}");
    assert!(
        text.contains("  \"Every test is seen failing before it passes.\""),
        "{text}"
    );
    assert!(text.contains("1 quoted line"), "{text}");
}
