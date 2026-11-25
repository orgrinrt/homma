//! An index brought current before it is asked, and one that cannot be, said behind and why.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use muisti::repo::Words;
use muisti::{Corpus, Embedder, Header, Store};

use super::current::Currency;
use super::discover::Place;
use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// An embedder of a model the verb does not bring.
struct Other;

impl Embedder for Other {
    fn model(&self) -> &str {
        "other"
    }

    fn dimension(&self) -> usize {
        1
    }

    fn embed(&mut self, _: &str, out: &mut [f32]) -> Result<(), muisti::Error> {
        out.fill(1.0);
        Ok(())
    }
}

/// The index of the corpus over `dir`, built through the walk by `embedder`, so its file records
/// the definition it was built from. It sits beside the corpus, not in it, so what the store
/// writes next to its file is no document of the corpus.
fn built<E: Embedder>(dir: &Path, embedder: &mut E) -> PathBuf {
    let file = dir.with_extension("index").join("index.sqlite");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let dimension = std::num::NonZeroUsize::new(embedder.dimension()).unwrap();
    let mut store = Store::open(&file, Header::new(embedder.model(), dimension)).unwrap();
    muisti::index(&mut store, &Corpus::new(dir), embedder, |_| {}).unwrap();
    file
}

fn line(out: &SearchOut) -> String {
    let mut text = Vec::new();
    out.render_human(&mut text).unwrap();
    String::from_utf8(text).unwrap()
}

#[test]
fn an_index_of_the_verbs_model_is_brought_current_before_it_is_asked() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("c");
    write(&dir.join("a.md"), "# A\n\nalpha\n");
    let file = built(&dir, &mut Words);
    write(&dir.join("b.md"), "# B\n\nsaturation of the air\n");
    let places = vec![Place::at("c".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    let paths: Vec<&str> = out.hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["b.md"],
        "the file written after the build answers"
    );
    assert!(
        matches!(
            out.indexes[0].currency,
            Some(Currency::Refreshed {
                added: 1,
                changed: 0,
                removed: 0,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    assert!(
        line(&out).contains("brought current: 1 added"),
        "{}",
        line(&out)
    );
    // THE CONTROL: asked again with nothing moved, it was current and its line says so.
    let again = ask(&places, "saturation", 10, SystemTime::now());
    assert!(line(&again).contains(", current"), "{}", line(&again));
    assert!(
        !line(&again).contains("brought current"),
        "{}",
        line(&again)
    );
}

#[test]
fn an_index_of_another_model_is_asked_as_it_stands_and_said_behind() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("c");
    write(&dir.join("a.md"), "# A\n\nsaturation alpha\n");
    let file = built(&dir, &mut Other);
    write(&dir.join("b.md"), "# B\n\nsaturation beta\n");
    let places = vec![Place::at("c".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    let paths: Vec<&str> = out.hits.iter().map(|h| h.path.as_str()).collect();
    // THE CONTROL: what the index held answers, so it was asked; what it did not hold does not.
    assert_eq!(paths, vec!["a.md"]);
    assert!(
        matches!(
            &out.indexes[0].currency,
            Some(Currency::Behind {
                files: 1,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    let text = line(&out);
    assert!(text.contains("behind by 1 file,"), "{text}");
    assert!(text.contains("`other`"), "{text}");
}

#[test]
fn an_index_recording_no_definition_says_how_far_behind_is_not_known() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("old.sqlite");
    let mut store = Store::open(&file, Header::new("words", std::num::NonZeroUsize::MIN)).unwrap();
    store
        .upsert(
            &muisti::Source::new("one.md", "# One\n\nsaturation\n".as_bytes()),
            &mut Words,
        )
        .unwrap();
    drop(store);
    let places = vec![Place::at("old".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    // THE CONTROL: it still answers from what it holds.
    assert_eq!(out.hits.len(), 1);
    assert!(
        matches!(out.indexes[0].currency, Some(Currency::Unknown { .. })),
        "{:?}",
        out.indexes[0].currency
    );
    assert!(
        line(&out).contains("how far behind is not known"),
        "{}",
        line(&out)
    );
}

#[test]
fn an_index_not_built_names_the_command_that_builds_it() {
    let t = tempfile::tempdir().unwrap();
    let gone = t.path().join("gone.sqlite");
    let named =
        Place::at("gone".to_string(), gone.clone()).built_by("`muisti index` in /c".to_string());
    let out = ask(&[named], "saturation", 10, SystemTime::now());
    assert!(!out.indexes[0].answered);
    assert_eq!(
        out.indexes[0].currency, None,
        "a file that is not there is not refreshed"
    );
    assert!(!gone.exists(), "the question built no index from nothing");
    let text = line(&out);
    assert!(text.contains("not built"), "{text}");
    assert!(text.contains("built by `muisti index` in /c"), "{text}");
    // THE CONTROL: a place that names no command says only that it is not built.
    let bare = ask(
        &[Place::at("gone".to_string(), gone)],
        "saturation",
        10,
        SystemTime::now(),
    );
    assert!(!line(&bare).contains("built by"), "{}", line(&bare));
}

#[test]
fn a_repository_index_is_brought_current_as_a_corpus_is() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("r");
    write(&dir.join("one.md"), "# One\n\nalpha\n");
    let file = t.path().join("r.git/mockspace/search.sqlite");
    let mut index = muisti::repo::RepoIndex::open(&file).unwrap();
    index
        .refresh(
            &[muisti::repo::Root::walked(&dir, "", muisti::repo::DocClass::Research)],
            None,
        )
        .unwrap();
    drop(index);
    write(&dir.join("two.md"), "# Two\n\nsaturation of the air\n");
    let places = vec![Place::at("r".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    let paths: Vec<&str> = out.hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["two.md"],
        "the document written after the build answers"
    );
    assert!(
        matches!(
            out.indexes[0].currency,
            Some(Currency::Refreshed {
                added: 1,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    // THE CONTROL: asked again with nothing moved, it was current.
    let again = ask(&places, "saturation", 10, SystemTime::now());
    assert!(line(&again).contains(", current"), "{}", line(&again));
}

#[test]
fn a_refresh_that_fails_gives_its_words_and_the_index_is_asked_as_it_stands() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("c");
    write(&dir.join("a.md"), "# A\n\nsaturation\n");
    let file = built(&dir, &mut Words);
    // The recorded root is gone, so the refresh under it cannot walk.
    std::fs::rename(&dir, t.path().join("moved")).unwrap();
    let places = vec![Place::at("c".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    // THE CONTROL: what the index held still answers.
    assert_eq!(out.hits.len(), 1);
    // With its root gone the measure cannot walk either, so how far behind is not known, and the
    // line gives both the refresh's words and the measure's.
    let currency = out.indexes[0].currency.clone().unwrap();
    assert!(matches!(currency, Currency::Unknown { .. }), "{currency:?}");
    let text = line(&out);
    assert!(text.contains("how far behind is not known"), "{text}");
    assert!(text.contains("; and measuring it: "), "{text}");
    assert!(
        text.contains(&dir.display().to_string()),
        "the crate's words name the root: {text}"
    );
}

#[test]
fn a_file_recording_no_definition_names_the_command_that_writes_one() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("old.sqlite");
    let mut store = Store::open(&file, Header::new("words", std::num::NonZeroUsize::MIN)).unwrap();
    store
        .upsert(
            &muisti::Source::new("one.md", "# One\n\nsaturation\n".as_bytes()),
            &mut Words,
        )
        .unwrap();
    drop(store);
    let named =
        Place::at("old".to_string(), file.clone()).built_by("`muisti index` in /c".to_string());
    let out = ask(&[named], "saturation", 10, SystemTime::now());
    assert!(
        line(&out).contains("`muisti index` in /c writes one"),
        "{}",
        line(&out)
    );
    // THE CONTROL: with no command known, none is named.
    let bare = ask(
        &[Place::at("old".to_string(), file)],
        "saturation",
        10,
        SystemTime::now(),
    );
    assert!(!line(&bare).contains("writes one"), "{}", line(&bare));
}

#[test]
fn what_a_refresh_skipped_is_a_note_on_the_line() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("c");
    write(&dir.join("a.md"), "# A\n\nsaturation\n");
    let file = built(&dir, &mut Words);
    std::fs::write(dir.join("c.bin"), [0xFF, 0xFE]).unwrap();
    let places = vec![Place::at("c".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    assert!(
        out.indexes[0]
            .notes
            .iter()
            .any(|n| n.contains("c.bin was skipped")),
        "{:?}",
        out.indexes[0].notes
    );
    // THE CONTROL: a refresh that skipped nothing leaves no such note.
    std::fs::remove_file(dir.join("c.bin")).unwrap();
    let again = ask(&places, "saturation", 10, SystemTime::now());
    assert!(
        !again.indexes[0]
            .notes
            .iter()
            .any(|n| n.contains("was skipped")),
        "{:?}",
        again.indexes[0].notes
    );
}

#[test]
fn an_index_of_another_model_recording_no_definition_names_the_command_that_writes_one() {
    // The crate refuses the refresh for its model first, and only the measure after finds the
    // file records no definition; the command is named all the same.
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("old.sqlite");
    let mut store = Store::open(&file, Header::new("other", std::num::NonZeroUsize::MIN)).unwrap();
    store
        .upsert(
            &muisti::Source::new("one.md", "# One\n\nsaturation\n".as_bytes()),
            &mut Other,
        )
        .unwrap();
    drop(store);
    let named =
        Place::at("old".to_string(), file.clone()).built_by("`muisti index` in /c".to_string());
    let out = ask(&[named], "saturation", 10, SystemTime::now());
    let text = line(&out);
    assert!(text.contains("`other`"), "{text}");
    assert!(text.contains("`muisti index` in /c writes one"), "{text}");
    // THE CONTROL: with no command known, none is named.
    let bare = ask(
        &[Place::at("old".to_string(), file)],
        "saturation",
        10,
        SystemTime::now(),
    );
    assert!(!line(&bare).contains("writes one"), "{}", line(&bare));
}

#[test]
fn every_count_on_the_line_is_said_in_its_number() {
    let behind = |files, commits| {
        Currency::Behind {
            files,
            commits,
            why: "why".to_string(),
        }
        .said()
    };
    assert!(
        behind(1, 0).starts_with("behind by 1 file,"),
        "{}",
        behind(1, 0)
    );
    assert!(
        behind(2, 0).starts_with("behind by 2 files,"),
        "{}",
        behind(2, 0)
    );
    assert!(
        behind(1, 1).starts_with("behind by 1 file and 1 commit,"),
        "{}",
        behind(1, 1)
    );
    assert!(
        behind(2, 2).starts_with("behind by 2 files and 2 commits,"),
        "{}",
        behind(2, 2)
    );
    let read = |commits| {
        Currency::Refreshed {
            added: 1,
            changed: 0,
            removed: 0,
            commits,
        }
        .said()
    };
    assert!(read(1).ends_with(", 1 commit read"), "{}", read(1));
    assert!(read(2).ends_with(", 2 commits read"), "{}", read(2));
    // THE CONTROL: nothing read, nothing said of commits.
    assert!(!read(0).contains("commit"), "{}", read(0));
}

/// The words embedder's model at a dimension the words embedder does not bring, so a writer of
/// the verb's embedder is refused the file for its header and not for its model.
struct WideWords;

impl Embedder for WideWords {
    fn model(&self) -> &str {
        Words.model()
    }

    fn dimension(&self) -> usize {
        2
    }

    fn embed(&mut self, _: &str, out: &mut [f32]) -> Result<(), muisti::Error> {
        out.fill(0.0);
        Ok(())
    }
}

#[test]
fn a_refresh_refused_while_the_measure_reads_is_said_behind_with_its_words() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("c");
    write(&dir.join("a.md"), "# A\n\nsaturation alpha\n");
    let file = built(&dir, &mut WideWords);
    write(&dir.join("b.md"), "# B\n\nsaturation beta\n");
    let out = ask(
        &[Place::at("c".to_string(), file)],
        "saturation",
        10,
        SystemTime::now(),
    );
    // THE CONTROL: what the index held answers, and the file written after it does not.
    let paths: Vec<&str> = out.hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["a.md"]);
    assert!(
        matches!(
            &out.indexes[0].currency,
            Some(Currency::Behind {
                files: 1,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    let text = line(&out);
    assert!(
        text.contains("behind by 1 file, not brought current: "),
        "{text}"
    );
    assert!(
        text.contains("dimension"),
        "the crate's words on the header: {text}"
    );
    assert!(
        !text.contains("its vectors are"),
        "refused for its header, not its model: {text}"
    );
}

#[test]
fn three_places_asked_together_each_carry_their_own_line() {
    let t = tempfile::tempdir().unwrap();
    let fresh = t.path().join("fresh");
    write(&fresh.join("a.md"), "# A\n\nalpha\n");
    let fresh_file = built(&fresh, &mut Words);
    write(&fresh.join("b.md"), "# B\n\nsaturation\n");
    let other = t.path().join("other");
    write(&other.join("a.md"), "# A\n\nsaturation\n");
    let other_file = built(&other, &mut Other);
    write(&other.join("b.md"), "# B\n\nbeta\n");
    let places = vec![
        Place::at("fresh".to_string(), fresh_file),
        Place::at("gone".to_string(), t.path().join("gone.sqlite"))
            .built_by("`muisti index` in /gone".to_string()),
        Place::at("other".to_string(), other_file),
    ];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    let names: Vec<&str> = out.indexes.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["fresh", "gone", "other"]);
    assert!(
        matches!(
            out.indexes[0].currency,
            Some(Currency::Refreshed {
                added: 1,
                ..
            })
        ),
        "fresh: {:?}",
        out.indexes[0].currency
    );
    assert_eq!(out.indexes[1].currency, None, "gone");
    assert!(
        out.indexes[1].notes.iter().any(|n| n.contains("/gone")),
        "gone: {:?}",
        out.indexes[1].notes
    );
    assert!(
        matches!(
            out.indexes[2].currency,
            Some(Currency::Behind {
                files: 1,
                ..
            })
        ),
        "other: {:?}",
        out.indexes[2].currency
    );
    // THE CONTROL: each outcome sits only on its own line.
    assert!(
        !out.indexes[0].notes.iter().any(|n| n.contains("/gone")),
        "{:?}",
        out.indexes[0].notes
    );
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn a_repository_index_reads_a_commit_made_after_its_build() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("r");
    write(&dir.join("one.md"), "# One\n\nalpha\n");
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "first"]);
    let file = t.path().join("r.index/search.sqlite");
    let roots = [muisti::repo::Root::walked(&dir, "", muisti::repo::DocClass::Research)];
    let mut index = muisti::repo::RepoIndex::open(&file).unwrap();
    index.refresh(&roots, Some(&dir)).unwrap();
    drop(index);
    write(&dir.join("two.md"), "# Two\n\nsaturation\n");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "second"]);
    let places = vec![Place::at("r".to_string(), file)];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    assert!(
        matches!(
            out.indexes[0].currency,
            Some(Currency::Refreshed {
                commits: 1 ..,
                ..
            })
        ),
        "{:?}",
        out.indexes[0].currency
    );
    assert!(line(&out).contains("1 commit read"), "{}", line(&out));
    // THE CONTROL: asked again with nothing committed, no commit is read.
    let again = ask(&places, "saturation", 10, SystemTime::now());
    assert!(!line(&again).contains("read"), "{}", line(&again));
}
