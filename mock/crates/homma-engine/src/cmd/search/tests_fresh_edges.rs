//! A listing at the edges: a day's boundary in UTC, an index with no history, places that cannot be
//! listed, the JSON, and the limit before the pass.

use std::cell::RefCell;

use homma_api::SummaryConfig;
use muisti::repo::{DocClass, RepoIndex, Root};

use super::discover::Place;
use super::tests_fresh::{NOW, answer_fresh, at, built, list, paths, planted, render, repository};
use super::*;

#[test]
fn a_commit_late_one_day_in_utc_is_a_day_old_just_after_midnight() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("r");
    repository(&dir, &[(
        "late.md",
        "# Late\n\njust before midnight\n",
        "2026-11-21T23:30:00Z",
    )]);
    let places = vec![Place::at("r".to_string(), built(&dir))];
    let now = at("2026-11-22T00:10:00Z");
    let two = list(&places, 2, None, now);
    let late = two.hits.iter().find(|h| h.path == "late.md").unwrap();
    assert_eq!(late.date.as_deref(), Some("2026-11-21"));
    assert_eq!(late.age.as_deref(), Some("1 day ago"));
    // THE CONTROL: forty minutes old, it is still yesterday's, so a window of today alone misses it.
    let one = list(&places, 1, None, now);
    assert!(one.hits.is_empty(), "{:?}", paths(&one));
}

#[test]
fn an_index_built_without_its_history_lists_its_files_and_no_commit() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("r");
    repository(&dir, &[(
        "new.md",
        "# New\n\nwritten yesterday\n",
        "2026-11-21T10:00:00Z",
    )]);
    // Built from the walk alone, with no repository to read a history from.
    let file = t.path().join("bare.index/search.sqlite");
    let mut index = RepoIndex::open(&file).unwrap();
    index
        .refresh(&[Root::walked(&dir, "", DocClass::Research)], None)
        .unwrap();
    drop(index);
    let bare = list(&[Place::at("bare".to_string(), file)], 30, None, at(NOW));
    assert!(bare.indexes[0].answered, "{:?}", bare.indexes[0].notes);
    assert!(
        bare.hits.iter().all(|h| !h.path.starts_with(".git/")),
        "{:?}",
        paths(&bare)
    );
    // THE CONTROL: the same repository indexed with its history lists its commit.
    let whole = list(
        &[Place::at("r".to_string(), built(&dir))],
        30,
        None,
        at(NOW),
    );
    assert!(
        whole.hits.iter().any(|h| h.path.starts_with(".git/")),
        "{:?}",
        paths(&whole)
    );
}

#[test]
fn a_place_missing_or_shut_keeps_its_line_and_is_not_listed() {
    let t = tempfile::tempdir().unwrap();
    let missing = Place::at("missing".to_string(), t.path().join("none/search.sqlite"))
        .built_by("`build it` in none".to_string());
    let shut = Place {
        shut: Some("no clone at elsewhere".to_string()),
        ..Place::at("shut".to_string(), t.path().join("elsewhere"))
    };
    let places = vec![missing, shut, planted(t.path(), "r")];
    let out = list(&places, 3, None, at(NOW));
    assert!(!out.indexes[0].answered);
    assert!(
        out.indexes[0]
            .notes
            .iter()
            .any(|n| n == "built by `build it` in none"),
        "{:?}",
        out.indexes[0].notes
    );
    assert!(!out.indexes[1].answered);
    assert_eq!(out.indexes[1].notes, vec![
        "no clone at elsewhere".to_string()
    ]);
    assert!(out.hits.iter().all(|h| h.index == "r"));
    // THE CONTROL: the place beside them is listed.
    assert!(out.indexes[2].answered);
    assert!(paths(&out).contains(&"new.md"));
}

#[test]
fn a_listing_carries_each_hits_age_in_its_json() {
    let t = tempfile::tempdir().unwrap();
    let place = planted(t.path(), "r");
    let out = list(std::slice::from_ref(&place), 3, None, at(NOW));
    let json = serde_json::to_value(&out).unwrap();
    let hits = json["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h["age"] == "1 day ago"), "{json}");
    // THE CONTROL: a question's hits carry no age.
    let asked = ask(std::slice::from_ref(&place), "note", 10, at(NOW));
    let json = serde_json::to_value(&asked).unwrap();
    let hits = json["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|h| h.get("age").is_none()), "{json}");
}

/// A model that keeps the content it was given and answers with one placed line.
struct Kept(RefCell<String>);

impl crate::cmd::summary::OneShot for Kept {
    fn ask(&self, _: &str, content: &str) -> Result<String, String> {
        *self.0.borrow_mut() = content.to_string();
        Ok("- new.md:1 a new note".to_string())
    }
}

#[test]
fn the_limit_bounds_what_the_pass_reads() {
    let t = tempfile::tempdir().unwrap();
    let places = vec![planted(t.path(), "r")];
    let on = SummaryConfig {
        enabled: true,
        ..SummaryConfig::default()
    };
    let model = Kept(RefCell::default());
    let all = list(&places, 30, None, at(NOW));
    assert_eq!(all.hits.len(), 4, "{:?}", paths(&all));
    let first = all.hits[0].path.clone();
    let last = all.hits[3].path.clone();
    answer_fresh(&places, 30, Some(1), Some((&on, &model)), at(NOW));
    let read = model.0.borrow().clone();
    assert!(read.contains(&first), "{read}");
    // THE CONTROL: the hits past the limit never reach the pass, and without it they do.
    assert!(!read.contains(&last), "{read}");
    answer_fresh(&places, 30, None, Some((&on, &model)), at(NOW));
    assert!(model.0.borrow().contains(&last), "{}", model.0.borrow());
    assert!(render(&all).contains(&last));
}
