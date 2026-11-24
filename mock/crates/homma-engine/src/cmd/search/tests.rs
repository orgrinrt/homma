//! Which indexes a planted workspace holds, and one question asked across them.

use std::num::NonZeroUsize;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use homma_api::SearchConfig;
use muisti::repo::Words;
use muisti::{Header, Source, Store};

use super::discover::{Place, content_name, narrowed, places};
use super::*;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A word index at `path` holding `docs`.
fn index(path: &Path, docs: &[(&str, &str)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut store = Store::open(path, Header::new("words", NonZeroUsize::MIN)).unwrap();
    for (p, text) in docs {
        store
            .upsert(&Source::new(*p, text.as_bytes()), &mut Words)
            .unwrap();
    }
}

#[test]
fn the_corpora_are_the_tracked_configs_each_at_the_index_it_names_or_the_default() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    git(root, &["init", "-q"]);
    write(&root.join("muisti.toml"), "muisti_branch = \"dev\"\n");
    write(
        &root.join("notes/muisti.toml"),
        "muisti_branch = \"dev\"\ncorpus.index = \"idx/notes.sqlite\"\n",
    );
    git(root, &["add", "muisti.toml", "notes/muisti.toml"]);
    // THE CONTROL: a config the repository does not track is nobody's corpus.
    write(&root.join("stray/muisti.toml"), "muisti_branch = \"dev\"\n");
    let got = places(root, "content", &SearchConfig::default(), []).unwrap();
    assert_eq!(got, vec![
        Place {
            name:  "content".to_string(),
            index: root.join(".muisti/index.sqlite"),
        },
        Place {
            name:  "content/notes".to_string(),
            index: root.join("notes/idx/notes.sqlite"),
        },
    ]);
}

#[test]
fn a_members_index_is_asked_only_where_the_manifest_opts_in_and_only_where_it_is_built() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("content");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    let built = t.path().join("built");
    let bare = t.path().join("bare");
    for m in [&built, &bare] {
        std::fs::create_dir_all(m).unwrap();
        git(m, &["init", "-q"]);
    }
    write(&built.join(".git/mockspace/search.sqlite"), "an index\n");
    let members = || vec![("built".to_string(), built.clone()), ("bare".to_string(), bare.clone())];
    // Off by default: the members are not the content repository's to open.
    let off = places(&root, "content", &SearchConfig::default(), members()).unwrap();
    assert!(off.is_empty());
    let on = SearchConfig {
        members: true,
        ..SearchConfig::default()
    };
    let got = places(&root, "content", &on, members()).unwrap();
    assert_eq!(got, vec![Place {
        name:  "built".to_string(),
        index: built.join(".git/mockspace/search.sqlite"),
    }]);
}

#[test]
fn a_question_narrowed_to_names_keeps_those_and_refuses_one_nothing_carries() {
    let all = vec![
        Place {
            name:  "a".to_string(),
            index: "a.sqlite".into(),
        },
        Place {
            name:  "b".to_string(),
            index: "b.sqlite".into(),
        },
    ];
    let one = narrowed(all.clone(), &["b".to_string()]).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].name, "b");
    assert_eq!(narrowed(all.clone(), &[]).unwrap().len(), 2);
    let err = narrowed(all, &["c".to_string()]).unwrap_err().to_string();
    assert!(err.contains("`c`") && err.contains("a, b"), "{err}");
}

#[test]
fn the_content_repository_is_named_by_its_last_segment_or_by_the_root() {
    let root = Path::new("/somewhere/work");
    assert_eq!(content_name(Some("content"), root), "content");
    assert_eq!(
        content_name(Some("git@forge.invalid:someone/content.git"), root),
        "content"
    );
    assert_eq!(
        content_name(Some("https://forge.invalid/someone/content/"), root),
        "content"
    );
    assert_eq!(content_name(Some("local"), root), "work");
    assert_eq!(content_name(None, root), "work");
}

#[test]
fn every_hit_names_its_index_and_an_index_not_built_is_a_line_under_the_answer() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a.sqlite");
    let b = t.path().join("b.sqlite");
    index(&a, &[("one.md", "# One\n\nthe saturation of the air\n")]);
    index(&b, &[("two.md", "# Two\n\nsaturation curves\n")]);
    let places = vec![
        Place {
            name:  "first".to_string(),
            index: a,
        },
        Place {
            name:  "gone".to_string(),
            index: t.path().join("gone.sqlite"),
        },
        Place {
            name:  "second".to_string(),
            index: b,
        },
    ];
    let out = ask(&places, "saturation", 10, SystemTime::now());
    let named: Vec<(&str, &str)> = out
        .hits
        .iter()
        .map(|h| (h.index.as_str(), h.path.as_str()))
        .collect();
    assert_eq!(named, vec![("first", "one.md"), ("second", "two.md")]);
    let answered: Vec<(&str, bool)> = out
        .indexes
        .iter()
        .map(|i| (i.name.as_str(), i.answered))
        .collect();
    assert_eq!(answered, vec![
        ("first", true),
        ("gone", false),
        ("second", true)
    ]);
    assert!(out.indexes[1].notes[0].contains("not built"));
    assert_eq!(out.indexes[0].model.as_deref(), Some("words"));
    let mut text = Vec::new();
    out.render_human(&mut text).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains("first   one.md:"), "{text}");
    assert!(text.contains("gone    not asked"), "{text}");
    // THE CONTROL: a question no index holds a word of finds nothing and says so.
    let none = ask(&places, "xylophone", 10, SystemTime::now());
    assert!(none.hits.is_empty());
    let mut text = Vec::new();
    none.render_human(&mut text).unwrap();
    assert!(
        String::from_utf8(text)
            .unwrap()
            .starts_with("nothing in any index answers")
    );
}
