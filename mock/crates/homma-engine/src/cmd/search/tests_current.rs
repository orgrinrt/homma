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
/// the definition it was built from.
fn built<E: Embedder>(dir: &Path, embedder: &mut E) -> PathBuf {
    let file = dir.join(".muisti/index.sqlite");
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
    assert!(text.contains("behind by 1 file"), "{text}");
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
