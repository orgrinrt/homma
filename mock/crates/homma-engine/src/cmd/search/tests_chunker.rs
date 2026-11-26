//! An index of another format than the one homma reads, cut by another chunker or recorded under
//! another extraction: not asked, and its line says which way it is out and what moves it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use muisti::repo::Words;
use muisti::repo::index::EXTRACTION;
use muisti::{CHUNKER_VERSION, Corpus, Embedder, Header, Store};

use super::discover::Place;
use super::*;

const BUILD: &str = "`cargo mock search --refresh` in m";

/// A corpus over `dir` whose index is cut under `chunker`, beside the corpus.
fn cut_by(dir: &Path, chunker: u32) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("a.md"), "# A\n\nsaturation of the air\n").unwrap();
    let file = dir.with_extension("index").join("index.sqlite");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let dimension = std::num::NonZeroUsize::new(Words.dimension()).unwrap();
    let mut header = Header::new(Words.model(), dimension);
    header.chunker = chunker;
    let mut store = Store::open(&file, header).unwrap();
    muisti::index(&mut store, &Corpus::new(dir), &mut Words, |_| {}).unwrap();
    file
}

/// A member's repository index over `dir`, built by the crate and then made to record `value`
/// for `key` in `table`, as a build of another format would have written it.
fn recorded(dir: &Path, table: &str, key: &str, value: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("a.md"), "# A\n\nsaturation of the air\n").unwrap();
    let file = dir.with_extension("git").join("mockspace/search.sqlite");
    let mut index = muisti::repo::RepoIndex::open(&file).unwrap();
    index
        .refresh(
            &[muisti::repo::Root::walked(dir, "", muisti::repo::DocClass::Research)],
            None,
        )
        .unwrap();
    drop(index);
    let conn = rusqlite::Connection::open(&file).unwrap();
    let changed = conn
        .execute(
            &format!("UPDATE {table} SET value = ?1 WHERE key = ?2"),
            rusqlite::params![value, key],
        )
        .unwrap();
    assert_eq!(changed, 1, "{table} records {key}");
    file
}

fn asked_at(file: PathBuf) -> IndexOut {
    let place = Place::at("m".to_string(), file).built_by(BUILD.to_string());
    let mut out = ask(&[place], "saturation", 10, SystemTime::now());
    out.indexes.remove(0)
}

fn asked(chunker: u32) -> IndexOut {
    let t = tempfile::tempdir().unwrap();
    asked_at(cut_by(&t.path().join("c"), chunker))
}

fn asked_member(table: &str, key: &str, value: impl ToString) -> IndexOut {
    let t = tempfile::tempdir().unwrap();
    asked_at(recorded(
        &t.path().join("r"),
        table,
        key,
        &value.to_string(),
    ))
}

/// The line of a file older than homma reads: the crate's words, then the command that writes it
/// again and the version homma reads.
fn says_older(line: &IndexOut, field: &str, reads: u32) {
    assert!(!line.answered);
    let notes = line.notes.join("\n");
    let refused = line
        .notes
        .iter()
        .position(|n| n.contains(&format!("{field} is")))
        .unwrap_or_else(|| panic!("the crate's words are kept: {notes}"));
    let named = line
        .notes
        .iter()
        .position(|n| n.contains(BUILD))
        .unwrap_or_else(|| panic!("the command is named: {notes}"));
    assert!(refused < named, "{notes}");
    assert!(
        line.notes[named].contains(&format!("homma reads {field} {reads}")),
        "{notes}"
    );
}

/// The line of a file newer than homma reads: no command, and the build of homma that reads it.
fn says_newer(line: &IndexOut, field: &str, reads: u32) {
    assert!(!line.answered);
    let notes = line.notes.join("\n");
    assert!(
        !notes.contains(BUILD),
        "its own command does not move it: {notes}"
    );
    assert!(
        notes.contains(&format!("this build of homma reads {field} {reads}")),
        "{notes}"
    );
    assert!(notes.contains("index crate's trunk"), "{notes}");
}

/// The line of a file of homma's own format: answered, no word on `field`.
fn says_nothing_of(line: &IndexOut, field: &str) {
    assert!(line.answered, "{:?}", line.notes);
    assert!(
        !line.notes.iter().any(|n| n.contains(field)),
        "{:?}",
        line.notes
    );
}

#[test]
fn an_index_of_the_chunker_homma_reads_answers_with_no_word_on_the_chunker() {
    // Failing control for the two below: the same index at the crate's own chunker answers.
    says_nothing_of(&asked(CHUNKER_VERSION), "chunker");
}

#[test]
fn an_index_cut_by_an_older_chunker_names_the_command_that_cuts_it_again() {
    says_older(&asked(CHUNKER_VERSION - 1), "chunker", CHUNKER_VERSION);
}

#[test]
fn an_index_cut_by_a_newer_chunker_says_this_build_of_homma_reads_an_older_one() {
    says_newer(&asked(CHUNKER_VERSION + 1), "chunker", CHUNKER_VERSION);
}

#[test]
fn a_member_index_cut_by_an_older_chunker_names_the_command_that_cuts_it_again() {
    // Failing control: the member's index as the crate writes it answers.
    says_nothing_of(
        &asked_member("header", "chunker", CHUNKER_VERSION),
        "chunker",
    );
    says_older(
        &asked_member("header", "chunker", CHUNKER_VERSION - 1),
        "chunker",
        CHUNKER_VERSION,
    );
}

#[test]
fn a_member_index_of_the_extraction_homma_reads_answers_with_no_word_on_the_extraction() {
    // Failing control for the two below.
    says_nothing_of(
        &asked_member("repo_meta", "extraction", EXTRACTION),
        "extraction",
    );
}

#[test]
fn a_member_index_of_an_older_extraction_names_the_command_that_writes_it_again() {
    says_older(
        &asked_member("repo_meta", "extraction", EXTRACTION - 1),
        "extraction",
        EXTRACTION,
    );
}

#[test]
fn a_member_index_of_a_newer_extraction_says_this_build_of_homma_reads_an_older_one() {
    says_newer(
        &asked_member("repo_meta", "extraction", EXTRACTION + 1),
        "extraction",
        EXTRACTION,
    );
}

#[test]
fn a_member_index_whose_extraction_is_no_number_gets_only_the_crates_words() {
    let line = asked_member("repo_meta", "extraction", "four");
    assert!(!line.answered);
    let notes = line.notes.join("\n");
    assert!(notes.contains("extraction is"), "{notes}");
    assert!(!notes.contains(BUILD), "{notes}");
    assert!(!notes.contains("homma reads"), "{notes}");
}
