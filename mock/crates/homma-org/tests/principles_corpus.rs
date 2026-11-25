//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Principles: a rule's frontmatter and a card, read from their own directory
//! beside the rules, and written as cards by the same renderer.

use std::fs;
use std::path::Path;

use homma_api::session::{Place, Session};
use homma_org::rules::{Corpus, CorpusError, Principles};

const KIND: Principles<'static> = Principles {
    card_prefix:  "principle-",
    title_prefix: "Principle: ",
};

fn rule(dir: &Path, name: &str) {
    fs::write(
        dir.join(format!("{name}.md.tmpl")),
        format!(
            "---\ntopics: [reuse]\nfires: \"always\"\nkind: reflex\n---\n\n# {name}\n\nCard.\n\n<!-- elaboration -->\n\nMore.\n"
        ),
    )
    .unwrap();
}

fn principle(dir: &Path, name: &str, body: &str) {
    fs::write(
        dir.join(format!("{name}.md.tmpl")),
        format!("---\ntopics: [reuse]\nfires: \"before writing\"\nkind: reflex\n---\n\n{body}"),
    )
    .unwrap();
}

/// A rules directory with one rule and a principles directory with one
/// principle and a settings file that is not one.
fn tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("rules")).unwrap();
    fs::create_dir_all(d.path().join("principles")).unwrap();
    rule(&d.path().join("rules"), "a-rule");
    principle(
        &d.path().join("principles"),
        "reuse",
        "# Principle: reuse\n\nUse what exists. Fires {{ fires }}.\n",
    );
    fs::write(d.path().join("principles/defaults.toml"), "[principles]\n").unwrap();
    d
}

fn load(d: &Path) -> Result<Corpus, CorpusError> {
    let mut c = Corpus::load(&d.join("rules"))?;
    c.load_principles(&d.join("principles"), &KIND)?;
    Ok(c)
}

/// Failing control: without `load_principles` no `principle-reuse.md` is written.
#[test]
fn a_principle_card_is_written_beside_the_rules_under_its_prefix() {
    let d = tree();
    let c = load(d.path()).unwrap();
    let out = d.path().join("cards");
    c.render_cards(&out, &Session::every(Place::Local)).unwrap();
    let card = fs::read_to_string(out.join("principle-reuse.md")).unwrap();
    assert_eq!(
        card,
        "# Principle: reuse\n\nUse what exists. Fires before writing.\n"
    );
    assert!(out.join("a-rule.md").is_file());
    assert!(
        c.unclaimed(&out).unwrap().is_empty(),
        "the principle's card is claimed"
    );
}

#[test]
fn a_principle_is_kept_apart_from_the_rules_and_about_answers_with_rules_only() {
    let d = tree();
    let c = load(d.path()).unwrap();
    assert_eq!(c.rules.len(), 1);
    assert_eq!(
        c.principles.len(),
        1,
        "the settings file is not a principle"
    );
    let hits: Vec<&str> = c
        .about("reuse")
        .iter()
        .map(|(r, _)| r.name.as_str())
        .collect();
    assert_eq!(hits, ["a-rule"]);
}

#[test]
fn a_principle_with_an_elaboration_is_refused() {
    let d = tree();
    principle(
        &d.path().join("principles"),
        "reuse",
        "# Principle: reuse\n\nCard.\n\n<!-- elaboration -->\n\nMore.\n",
    );
    match load(d.path()).unwrap_err() {
        CorpusError::Principle {
            reason,
            ..
        } => assert!(reason.contains("elaboration"), "{reason}"),
        other => panic!("wrong refusal: {other}"),
    }
}

#[test]
fn a_principle_whose_first_heading_is_not_its_title_is_refused() {
    let d = tree();
    principle(&d.path().join("principles"), "reuse", "# Reuse\n\nCard.\n");
    match load(d.path()).unwrap_err() {
        CorpusError::Principle {
            reason,
            ..
        } => assert!(reason.contains("`# Principle: reuse`"), "{reason}"),
        other => panic!("wrong refusal: {other}"),
    }
}

/// Failing control: without the heading-of-any-level check, `## Reuse` before
/// the title passes as no title at all being found first.
#[test]
fn a_principle_whose_first_heading_is_of_another_level_is_refused() {
    let d = tree();
    principle(
        &d.path().join("principles"),
        "reuse",
        "## Reuse\n\n# Principle: reuse\n\nCard.\n",
    );
    assert!(matches!(load(d.path()), Err(CorpusError::Principle { .. })));
}

/// Failing control: without the clash check both load and the rule's card is
/// overwritten by the principle's.
#[test]
fn a_principle_whose_card_a_rule_already_writes_is_refused() {
    let d = tree();
    rule(&d.path().join("rules"), "principle-reuse");
    match load(d.path()).unwrap_err() {
        CorpusError::Principle {
            reason,
            ..
        } => assert!(reason.contains("`principle-reuse.md`"), "{reason}"),
        other => panic!("wrong refusal: {other}"),
    }
}
