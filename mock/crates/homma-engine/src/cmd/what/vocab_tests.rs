//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::answer;
use super::doc::{Doc, Kind};
use super::text::{Grade, grade, stem};
use super::vocab::Vocabulary;

fn groups(gs: &[&[&str]]) -> Vec<Vec<String>> {
    gs.iter()
        .map(|g| g.iter().map(|w| (*w).to_string()).collect())
        .collect()
}

fn same(v: &Vocabulary, a: &str, b: &str) -> bool {
    grade(&stem(a), &stem(b), v) == Some(Grade::Synonym)
}

#[test]
fn a_workspace_group_sharing_a_word_with_the_table_joins_its_group() {
    let v = Vocabulary::with(&groups(&[&["courier", "message"]]));
    assert!(same(&v, "courier", "notify"));
    // Failing control: the compiled table alone does not know the word.
    assert_eq!(
        grade(&stem("courier"), &stem("notify"), Vocabulary::compiled()),
        None
    );
}

#[test]
fn a_workspace_group_of_words_the_table_lacks_is_a_group_of_its_own() {
    let v = Vocabulary::with(&groups(&[&["kettle", "teapot"]]));
    assert!(same(&v, "kettle", "teapot"));
    assert!(!same(&v, "kettle", "notify"));
    assert!(same(&v, "stop", "end"), "the compiled groups stay");
}

#[test]
fn two_workspace_groups_bridged_by_one_word_are_one_group() {
    let v = Vocabulary::with(&groups(&[&["kettle", "boiler"], &["boilers", "urn"]]));
    assert!(same(&v, "kettle", "urn"));
}

#[test]
fn an_answer_matches_a_thing_by_a_workspace_synonym() {
    let docs = vec![Doc::new(Kind::Tool, "brew", "brew", "heats the kettle for the team")];
    let v = Vocabulary::with(&groups(&[&["kettle", "teapot"]]));
    let hits = |a: super::Answer| a.hits.iter().map(|h| h.name.clone()).collect::<Vec<_>>();
    assert_eq!(hits(answer(&docs, Vec::new(), "teapot", 5, &v)), ["brew"]);
    // Failing control: with the compiled table alone the word answers nothing.
    assert!(
        hits(answer(
            &docs,
            Vec::new(),
            "teapot",
            5,
            Vocabulary::compiled()
        ))
        .is_empty()
    );
}

fn manifest(body: &str) -> homma_core::Config {
    homma_core::Config::parse(&format!("[workspace]\nname = \"w\"\n{body}")).unwrap()
}

#[test]
fn the_manifest_s_groups_are_read_and_with_none_the_compiled_table_answers() {
    let v = Vocabulary::of(Some(&manifest(
        "[what]\nsynonyms = [[\"kettle\", \"teapot\"]]\n",
    )))
    .unwrap();
    assert!(same(&v, "kettle", "teapot"));
    assert_eq!(
        &Vocabulary::of(Some(&manifest(""))).unwrap(),
        Vocabulary::compiled()
    );
    assert_eq!(&Vocabulary::of(None).unwrap(), Vocabulary::compiled());
}

#[test]
fn the_verb_fails_on_a_key_the_what_table_does_not_know() {
    let t = tempfile::tempdir().unwrap();
    let run = |body: &str| {
        super::run(
            t.path(),
            Some(&manifest(body)),
            &["kettle".to_string()],
            5,
            &[],
            false,
            crate::cli::OutputFormat::Json,
        )
    };
    // Failing control: the key spelt right reads, so the refusal is the misspelling's.
    assert!(run("[what]\nsynonyms = [[\"kettle\", \"teapot\"]]\n").is_ok());
    let Err(refused) = run("[what]\nsynonym = [[\"kettle\", \"teapot\"]]\n") else {
        panic!("a misspelt key fails the verb");
    };
    let why = format!("{refused:#}");
    assert!(why.contains("`[what]`"), "{why}");
    assert!(why.contains("synonym"), "{why}");
}
