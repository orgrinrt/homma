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

#[test]
fn the_manifest_s_groups_are_read_and_a_what_table_that_does_not_read_is_a_gap() {
    let t = tempfile::tempdir().unwrap();
    let manifest = t.path().join("homma.toml");
    std::fs::write(
        &manifest,
        "[workspace]\nname = \"w\"\n[what]\nsynonyms = [[\"kettle\", \"teapot\"]]\n",
    )
    .unwrap();
    let (v, gap) = Vocabulary::of_manifest(&manifest);
    assert!(gap.is_none());
    assert!(same(&v, "kettle", "teapot"));

    std::fs::write(
        &manifest,
        "[workspace]\nname = \"w\"\n[what]\nsynonym = [[\"kettle\", \"teapot\"]]\n",
    )
    .unwrap();
    let (v, gap) = Vocabulary::of_manifest(&manifest);
    let gap = gap.expect("a misspelt key is said");
    assert!(gap.what.contains("[what] does not read"), "{}", gap.what);
    assert_eq!(&v, Vocabulary::compiled());

    std::fs::write(&manifest, "[workspace]\nname = \"w\"\n").unwrap();
    let (v, gap) = Vocabulary::of_manifest(&manifest);
    assert!(gap.is_none());
    assert_eq!(&v, Vocabulary::compiled());

    let (v, gap) = Vocabulary::of_manifest(&t.path().join("none.toml"));
    assert!(gap.is_none());
    assert_eq!(&v, Vocabulary::compiled());
}
