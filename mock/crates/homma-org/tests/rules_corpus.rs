//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Loading a rule corpus, finding a rule by subject, and generating its card.
//!
//! A rule is one file: meta, then the card, then a marker, then the
//! elaboration. The card is a prefix of the full rule, so most of what is worth
//! testing is that the split lands in the right place and that nothing from one
//! side leaks into the other.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_org::rules::{Corpus, CorpusError};

/// A corpus of three rules on disjoint subjects.
///
/// The directory is unique per call, not per process. The tests in one binary
/// are threads sharing a process id, so keying on that gave every test the same
/// directory and each one's setup deleted the others' out from under them.
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

/// A generated card carries its rule's path gating.
///
/// The host reads `paths:` off the generated file to decide whether a rule is
/// injected always or only against matching files. A pass that renders the body
/// alone promotes every gated rule to always-loaded, and the only sign is a
/// count going up somewhere nobody was looking.
#[test]
fn a_gated_rule_keeps_its_gating_through_generation() {
    let dir = fixture();
    let src = dir.join("rules");
    fs::write(
        src.join("rust-only.md.tmpl"),
        "---\ntopics: [rust]\nfires: \"in rust\"\nkind: reflex\npaths: [\"**/*.rs\", \"build.rs\"]\n\
         ---\n\n# Rust only\n\nThe move.\n",
    )
    .unwrap();

    let out = dir.join("out");
    Corpus::load(&src).unwrap().render_cards(&out).unwrap();

    let card = fs::read_to_string(out.join("rust-only.md")).unwrap();
    assert!(
        card.starts_with("---\npaths:\n"),
        "the gating opens the generated card: {card}"
    );
    assert!(card.contains("  - \"**/*.rs\"\n"), "{card}");
    assert!(card.contains("  - \"build.rs\"\n"), "{card}");
    assert!(card.contains("# Rust only"), "and the body follows: {card}");

    // The control: a rule with no paths gets no block, else every card would
    // open with an empty one and the host would read that as gated on nothing.
    let plain = fs::read_to_string(out.join("writing-style.md")).unwrap();
    assert!(
        !plain.starts_with("---"),
        "an ungated rule opens with its heading: {plain}"
    );
}

/// One rule file: meta, card, marker, elaboration.
fn write(dir: &Path, name: &str, topics: &[&str], fires: &str) {
    let topics = topics.join(", ");
    fs::write(
        dir.join(format!("{name}.md.tmpl")),
        format!(
            "---\ntopics: [{topics}]\nfires: \"{fires}\"\nkind: reflex\n---\n\n\
             # {name}\n\nThe absolute, stated once.\n\nFires {{{{ fires }}}}.\n\n\
             <!-- elaboration -->\n\nThe whole reasoning for {name}.\n"
        ),
    )
    .unwrap();
}

#[test]
fn loads_every_rule_and_splits_it_at_the_marker() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    assert_eq!(c.rules.len(), 3);
    let w = c.rules.iter().find(|r| r.name == "writing-style").unwrap();
    assert_eq!(w.meta.topics, vec!["writing", "prose", "readme"]);
    assert!(w.card.contains("The absolute, stated once"));
    assert!(w.elaboration.contains("The whole reasoning"));
    // Neither half carries the other, which is the whole point of the split.
    assert!(
        !w.card.contains("The whole reasoning"),
        "card ran past the marker"
    );
    assert!(
        !w.elaboration.contains("The absolute"),
        "elaboration ran back before it"
    );
    assert!(
        !w.card.contains("<!-- elaboration -->"),
        "the marker leaked into the card"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_rule_with_no_marker_is_all_card_and_is_not_a_defect() {
    // The shape of a rule whose whole statement fits. Reporting it would refuse
    // most of the corpus for being short enough not to need a second half.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("short.md.tmpl"),
        "---\ntopics: [brief]\nfires: \"always\"\nkind: reflex\n---\n\n# Short\n\nAll of it.\n",
    )
    .unwrap();
    let c = Corpus::load(&src).unwrap();
    let r = c.rules.iter().find(|r| r.name == "short").unwrap();
    assert!(r.card.contains("All of it."));
    assert!(r.elaboration.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn about_returns_only_the_rules_that_answer() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let hits = c.about("writing, readme");
    assert_eq!(hits.len(), 1);
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
    let names = |c: &Corpus| -> Vec<String> {
        c.about("shared")
            .iter()
            .map(|(r, _)| r.name.clone())
            .collect()
    };
    assert_eq!(names(&c), names(&c));
    assert_eq!(names(&c), vec![
        "aaa-rule".to_string(),
        "zzz-rule".to_string()
    ]);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn renders_the_card_and_not_the_elaboration() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let out = d.join("cards");
    assert_eq!(c.render_cards(&out).unwrap().len(), 3);

    let card = fs::read_to_string(out.join("writing-style.md")).unwrap();
    assert!(card.contains("# writing-style"));
    assert!(
        card.contains("before writing prose a human reads"),
        "the trigger is written once in the meta and rendered into the card: {card}"
    );
    assert!(
        !card.contains("The whole reasoning"),
        "the elaboration must not reach the always-loaded card: {card}"
    );
    // Another rule's meta must not leak in through a shared environment.
    assert!(
        !card.contains("pull request"),
        "card carried another rule's meta: {card}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_generated_card_ends_with_exactly_one_newline() {
    // It is injected into a session next to others, so a missing trailing
    // newline runs two rules together and a pile of them adds blank lines.
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let out = d.join("cards");
    c.render_cards(&out).unwrap();
    let card = fs::read_to_string(out.join("writing-style.md")).unwrap();
    assert!(card.ends_with('\n'));
    assert!(!card.ends_with("\n\n"));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn the_full_rule_carries_both_halves_in_order() {
    let d = fixture();
    let c = Corpus::load(&d.join("rules")).unwrap();
    let r = c.rules.iter().find(|r| r.name == "writing-style").unwrap();
    let full = c.render_full(r).unwrap();
    let card_at = full
        .find("The absolute, stated once")
        .expect("card is present");
    let elab_at = full
        .find("The whole reasoning")
        .expect("elaboration is present");
    assert!(card_at < elab_at, "the card is the prefix, not the suffix");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_template_can_say_which_variant_it_is_being_rendered_as() {
    // The few lines that differ: a card ends by naming the fetch, and that
    // sentence is noise in the full rule where the reader is already past it.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("writing-style.md.tmpl"),
        "---\ntopics: [writing]\nfires: \"x\"\nkind: reflex\n---\n\n# W\n\nBody.\n\
         {% if variant == \"card\" %}\nFetch the rest.\n{% endif %}\n\
         <!-- elaboration -->\n\nDepth.\n",
    )
    .unwrap();
    let c = Corpus::load(&src).unwrap();
    let r = c.rules.iter().find(|r| r.name == "writing-style").unwrap();
    let out = d.join("cards");
    c.render_cards(&out).unwrap();
    let card = fs::read_to_string(out.join("writing-style.md")).unwrap();
    assert!(card.contains("Fetch the rest"));
    assert!(!c.render_full(r).unwrap().contains("Fetch the rest"));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_rule_whose_meta_will_not_parse_refuses_the_whole_load_and_names_the_file() {
    // Dropping it instead would report a smaller, healthier corpus than the one
    // on disk, which passes every check and governs less than it claims.
    let d = fixture();
    let src = d.join("rules");
    fs::write(src.join("broken.md.tmpl"), "no frontmatter here\n").unwrap();
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
fn a_template_naming_something_the_meta_does_not_have_is_refused() {
    // Strict undefined handling is why the engine is configured that way: a
    // typo would otherwise render an empty string into every session, silently.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("writing-style.md.tmpl"),
        "---\ntopics: [a]\nfires: \"x\"\nkind: reflex\n---\n\n{{ nonexistent_field }}\n",
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

#[test]
fn a_file_with_a_further_extension_is_not_a_rule() {
    // A rule's name is a slug. Without the guard `x.card.md.tmpl` strips to a
    // rule named `x.card`, which then fails to parse and refuses the whole
    // corpus for a file that was never a rule.
    let d = fixture();
    let src = d.join("rules");
    fs::write(
        src.join("writing-style.card.md.tmpl"),
        "not a rule at all\n",
    )
    .unwrap();
    let c = Corpus::load(&src).expect("a neighbouring file must not refuse the load");
    assert_eq!(c.rules.len(), 3);
    assert!(!c.rules.iter().any(|r| r.name.contains('.')));
    let _ = fs::remove_dir_all(&d);
}
