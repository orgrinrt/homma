//! An index cut by another chunker than the one homma reads: not asked, and its line says which way
//! it is out and what moves it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use muisti::repo::Words;
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

fn asked(chunker: u32) -> IndexOut {
    let t = tempfile::tempdir().unwrap();
    let file = cut_by(&t.path().join("c"), chunker);
    let place = Place::at("m".to_string(), file).built_by(BUILD.to_string());
    let mut out = ask(&[place], "saturation", 10, SystemTime::now());
    out.indexes.remove(0)
}

#[test]
fn an_index_of_the_chunker_homma_reads_answers_with_no_word_on_the_chunker() {
    // Failing control for the two below: the same index at the crate's own chunker answers.
    let line = asked(CHUNKER_VERSION);
    assert!(line.answered, "{:?}", line.notes);
    assert!(
        !line.notes.iter().any(|n| n.contains("chunker")),
        "{:?}",
        line.notes
    );
}

#[test]
fn an_index_cut_by_an_older_chunker_names_the_command_that_cuts_it_again() {
    let line = asked(CHUNKER_VERSION - 1);
    assert!(!line.answered);
    let notes = line.notes.join("\n");
    let refused = line
        .notes
        .iter()
        .position(|n| n.contains("chunker is"))
        .expect("the crate's words are kept");
    let named = line
        .notes
        .iter()
        .position(|n| n.contains(BUILD))
        .unwrap_or_else(|| panic!("the command is named: {notes}"));
    assert!(refused < named, "{notes}");
    assert!(
        line.notes[named].contains(&format!("homma reads chunker {CHUNKER_VERSION}")),
        "{notes}"
    );
}

#[test]
fn an_index_cut_by_a_newer_chunker_says_this_build_of_homma_reads_an_older_one() {
    let line = asked(CHUNKER_VERSION + 1);
    assert!(!line.answered);
    let notes = line.notes.join("\n");
    assert!(
        !notes.contains(BUILD),
        "its own command does not move it: {notes}"
    );
    assert!(
        notes.contains(&format!(
            "this build of homma reads chunker {CHUNKER_VERSION}"
        )),
        "{notes}"
    );
    assert!(notes.contains("index crate's trunk"), "{notes}");
}
