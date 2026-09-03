//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Loading a rule corpus, finding a rule by subject, and generating its card.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_org::rules::{Corpus, CorpusError};

/// A corpus of three rules on disjoint subjects, each with a card.
///
/// The directory is unique per call, not per process. The tests in one binary
/// are threads sharing a process id, so keying on that gave every test the same
/// directory and each one's setup deleted the others' out from under them: nine
/// of eleven failed, on races rather than on anything they were testing.
fn fixture() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-rules-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("rules");
    fs::create_dir_all(&src).unwrap();

    write(
        &src,
        "writing-style",
        &["writing", "prose", "readme"],
        "before writing prose a human reads",
    );
    write(
        &src,
        "branch-flow",
        &["git", "branch", "pull-request"],
        "before opening a pull request",
    );
    write(
        &src,
        "test-gate",
        &["tests", "suite"],
        "before trusting a green suite",
    );
    dir
}

fn write(dir: &Path, name: &str, topics: &[&str], fires: &str) {
    let topics = topics.join(", ");
    fs::write(
        dir.join(format!("{name}.full.md.tmpl")),
        format!("---\ntopics: [{topics}]\nfires: \"{fires}\"\nkind: reflex\n---\n\nThe whole reasoning for {name}.\n"),
    )
    .unwrap();
    fs::write(
        dir.join(format!("{name}.card.md.tmpl")),
        "# {{ name }}\n\nFires when {{ fires }}.\n\n`rules show {{ name }}` for the rest.\n",
    )
    .unwrap();
}

#[test]
fn loads_every_rule_with_its_meta_and_body() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    assert_eq!(c.rules.len(), 3);
    let w = c.rules.iter().find(|r| r.name == "writing-style").unwrap();
    assert_eq!(w.meta.topics, vec!["writing", "prose", "readme"]);
    assert!(w.body.contains("The whole reasoning"));
    assert!(w.card.is_some(), "the card beside it must be picked up");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn about_returns_only_the_rules_that_answer() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let hits = c.about("writing, readme");
    assert_eq!(hits.len(), 1, "only one rule is about writing");
    assert_eq!(hits[0].0.name, "writing-style");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn about_returns_nothing_for_a_subject_the_corpus_does_not_cover() {
    // The control. Without it a search returning everything would satisfy the
    // case above, and every query would come back with the whole corpus.
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    assert!(c.about("vulkan, shaders").is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn about_ranks_the_rule_answering_more_of_the_query_first() {
    let d = fixture();
    let src = d.join("rules");
    // A second rule sharing one term with the first, so the ordering is decided
    // by how much of the query each answers rather than by filename.
    write(
        &src,
        "readme-format",
        &["readme"],
        "before writing a readme",
    );
    let c = Corpus::load(&src).unwrap();
    let hits = c.about("writing, prose, readme");
    assert_eq!(hits[0].0.name, "writing-style", "three terms beats one");
    assert!(hits.iter().any(|(r, _)| r.name == "readme-format"));
    assert!(hits[0].1 > hits[1].1);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn about_is_stable_between_runs_when_scores_tie() {
    // Two rules answering equally must not swap order run to run, or a diff of
    // the output reports a change nobody made.
    let d = fixture();
    let src = d.join("rules");
    write(&src, "aaa-rule", &["shared"], "x");
    write(&src, "zzz-rule", &["shared"], "x");
    let c = Corpus::load(&src).unwrap();
    let first: Vec<_> = c
        .about("shared")
        .iter()
        .map(|(r, _)| r.name.clone())
        .collect();
    let second: Vec<_> = c
        .about("shared")
        .iter()
        .map(|(r, _)| r.name.clone())
        .collect();
    assert_eq!(first, second);
    assert_eq!(first, vec!["aaa-rule".to_string(), "zzz-rule".to_string()]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn renders_a_card_per_rule_against_its_own_meta() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let out = d.join("cards");
    let written = c.render_cards(&out).unwrap();
    assert_eq!(written.len(), 3);

    let card = fs::read_to_string(out.join("writing-style.md")).unwrap();
    assert!(card.contains("# writing-style"));
    assert!(
        card.contains("before writing prose a human reads"),
        "the trigger is written once in the meta and appears in the card: {card}"
    );
    assert!(card.contains("rules show writing-style"));
    // The other rule's trigger must not leak in through a shared environment.
    assert!(
        !card.contains("pull request"),
        "card carried another rule's meta: {card}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_elaboration_with_no_card_is_refused_rather_than_skipped() {
    // Skipping leaves a rule authored, findable by `about`, and absent from
    // every session that was supposed to load it, with nothing reporting so.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("orphan.full.md.tmpl"),
        "---\ntopics: [a]\nfires: \"x\"\nkind: reflex\n---\n\nbody\n",
    )
    .unwrap();
    let c = Corpus::load(&src).unwrap();
    match c.render_cards(&d.join("cards")) {
        Err(CorpusError::NoCard {
            name,
        }) => assert_eq!(name, "orphan"),
        other => panic!("expected a refusal naming the rule, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_rule_whose_meta_will_not_parse_refuses_the_whole_load_and_names_the_file() {
    // Dropping it instead would report a smaller, healthier corpus than the one
    // on disk, which passes every check and governs less than it claims.
    let d = fixture();
    let src = d.join("rules");
    fs::write(src.join("broken.full.md.tmpl"), "no frontmatter here\n").unwrap();
    match Corpus::load(&src) {
        Err(CorpusError::Meta {
            path,
            ..
        }) => assert!(path.to_string_lossy().contains("broken")),
        other => panic!("expected a refusal naming the file, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_card_naming_something_the_meta_does_not_have_is_refused() {
    // Strict undefined handling is the reason the engine is configured that
    // way: a typo in a card would otherwise render an empty string into every
    // session, silently.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("writing-style.card.md.tmpl"),
        "# {{ name }}\n\n{{ nonexistent_field }}\n",
    )
    .unwrap();
    let c = Corpus::load(&src).unwrap();
    match c.render_cards(&d.join("cards")) {
        Err(CorpusError::Render {
            ..
        }) => {},
        other => panic!("expected a render refusal, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_card_may_carry_the_whole_body() {
    // Some rules are broken by people who have read them, and a card that only
    // points at the reasoning stops nothing. Those carry it, so `body` is in
    // the context on purpose.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("writing-style.card.md.tmpl"),
        "# {{ name }}\n\n{{ body }}\n",
    )
    .unwrap();
    let c = Corpus::load(&src).unwrap();
    c.render_cards(&d.join("cards")).unwrap();
    let card = fs::read_to_string(d.join("cards").join("writing-style.md")).unwrap();
    assert!(card.contains("The whole reasoning for writing-style"));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_missing_directory_is_refused_rather_than_read_as_an_empty_corpus() {
    // An empty corpus passes every check and generates no cards, so it looks
    // like a workspace with no rules rather than a path that is wrong.
    let d = fixture();
    match Corpus::load(&d.join("no-such-dir")) {
        Err(CorpusError::Unreadable {
            ..
        }) => {},
        other => panic!("expected a refusal, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&d);
}
