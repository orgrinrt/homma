//! Which indexes a planted workspace holds, and one question asked across them.

use std::num::NonZeroUsize;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use clap::Parser;
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
        Place::at("content".to_string(), root.join(".muisti/index.sqlite")),
        Place::at(
            "content/notes".to_string(),
            root.join("notes/idx/notes.sqlite")
        ),
    ]);
}

#[test]
fn a_corpus_index_leaving_the_repository_is_refused_and_a_config_that_does_not_read_is_a_note() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    git(root, &["init", "-q"]);
    write(
        &root.join("up/muisti.toml"),
        "corpus.index = \"../../outside.sqlite\"\n",
    );
    write(
        &root.join("abs/muisti.toml"),
        "corpus.index = \"/elsewhere/index.sqlite\"\n",
    );
    write(&root.join("bad/muisti.toml"), "corpus.index = [\n");
    // THE CONTROL: climbing out of its own directory and staying in the repository is the config's
    // to do.
    write(
        &root.join("near/muisti.toml"),
        "corpus.index = \"../shared/near.sqlite\"\n",
    );
    git(root, &["add", "."]);
    let got = places(root, "content", &SearchConfig::default(), []).unwrap();
    let by = |n: &str| got.iter().find(|p| p.name == n).unwrap();
    for refused in ["content/up", "content/abs"] {
        let why = by(refused).shut.as_deref().unwrap_or_default();
        assert!(
            why.contains("leaves the content repository"),
            "{refused}: {why}"
        );
    }
    let near = by("content/near");
    assert_eq!(near.shut, None);
    assert_eq!(near.index, root.join("near/../shared/near.sqlite"));
    let bad = by("content/bad");
    assert_eq!(bad.shut, None);
    assert_eq!(bad.index, root.join("bad/.muisti/index.sqlite"));
    assert!(bad.notes[0].contains("does not read"), "{:?}", bad.notes);
    // THE CONTROL: a config that reads carries no note.
    assert!(near.notes.is_empty());
    // Asked, the refused corpus is a line saying why and is never opened.
    let out = ask(&got, "anything", 10, SystemTime::now());
    let up = out.indexes.iter().find(|i| i.name == "content/up").unwrap();
    assert!(!up.answered);
    assert!(
        up.notes
            .iter()
            .any(|n| n.contains("leaves the content repository"))
    );
    let bad = out
        .indexes
        .iter()
        .find(|i| i.name == "content/bad")
        .unwrap();
    assert!(bad.notes.iter().any(|n| n.contains("does not read")));
}

#[test]
fn a_member_asked_for_is_a_line_whether_its_index_is_built_or_its_clone_is_there() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("content");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    let built = t.path().join("built");
    let bare = t.path().join("bare");
    let absent = t.path().join("absent");
    for m in [&built, &bare] {
        std::fs::create_dir_all(m).unwrap();
        git(m, &["init", "-q"]);
    }
    index(&built.join(".git/mockspace/search.sqlite"), &[(
        "one.md",
        "# One\n\nsaturation\n",
    )]);
    let members = || {
        vec![
            ("built".to_string(), built.clone()),
            ("bare".to_string(), bare.clone()),
            ("absent".to_string(), absent.clone()),
        ]
    };
    // Off by default: the members are not the content repository's to open.
    let off = places(&root, "content", &SearchConfig::default(), members()).unwrap();
    assert!(off.is_empty());
    let on = SearchConfig {
        members: true,
        ..SearchConfig::default()
    };
    let got = places(&root, "content", &on, members()).unwrap();
    let names: Vec<&str> = got.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["built", "bare", "absent"]);
    assert_eq!(got[1].index, bare.join(".git/mockspace/search.sqlite"));
    assert_eq!(got[1].shut, None);
    assert!(
        got[2]
            .shut
            .as_deref()
            .is_some_and(|w| w.contains("no clone"))
    );
    let out = ask(&got, "saturation", 10, SystemTime::now());
    let lines: Vec<(&str, bool)> = out
        .indexes
        .iter()
        .map(|i| (i.name.as_str(), i.answered))
        .collect();
    // THE CONTROL: the built one answers, so the other two are lines for not being there.
    assert_eq!(lines, vec![
        ("built", true),
        ("bare", false),
        ("absent", false)
    ]);
    assert!(
        out.indexes[1].notes[0].contains("not built"),
        "{:?}",
        out.indexes[1].notes
    );
    // The line is the place's own reason, with no prefix of the index crate's error that stands in.
    assert_eq!(
        out.indexes[2].notes.first(),
        got[2].shut.as_ref(),
        "{:?}",
        out.indexes[2].notes
    );
    assert!(
        out.indexes[2].notes[0].starts_with("no clone"),
        "{:?}",
        out.indexes[2].notes
    );
    assert!(!out.indexes[2].notes[0].contains("cache:"));
    assert_eq!(out.hits.len(), 1);
}

/// A manifest carrying `body` beside the workspace name every manifest has.
fn manifest(body: &str) -> homma_core::Config {
    homma_core::Config::parse(&format!("[workspace]\nname = \"w\"\n{body}")).unwrap()
}

#[test]
fn the_members_flag_asks_the_members_for_one_run_whatever_the_manifest_says() {
    use super::discover::search_config;
    // THE CONTROL: without the flag the default is off, so the flag is what turns it on.
    assert!(!search_config(&manifest(""), false).unwrap().members);
    assert!(search_config(&manifest(""), true).unwrap().members);
    // The flag only adds: a manifest that opts in stays in without it.
    let opted = manifest("[search]\nmembers = true\n");
    assert!(search_config(&opted, false).unwrap().members);
    // The flag reaches the command as `members`.
    let cli =
        crate::cli::Cli::try_parse_from(["homma-engine", "search", "--members", "word"]).unwrap();
    assert!(matches!(cli.command, crate::cli::Command::Search {
        members: true,
        ..
    }));
}

#[test]
fn the_search_table_refuses_a_key_it_does_not_read_and_a_value_of_the_wrong_type() {
    use super::discover::search_config;
    let read = search_config(
        &manifest("[search]\ncorpus_index = \"idx/one.sqlite\"\n"),
        false,
    )
    .unwrap();
    // THE CONTROL: a key it reads, with a value of its type, is taken.
    assert_eq!(read.corpus_index.as_path(), Path::new("idx/one.sqlite"));
    for body in [
        "[search]\nmembres = true\n",
        "[search]\nmembers = \"yes\"\n",
        "[search]\ncorpus_index = \"../out.sqlite\"\n",
    ] {
        assert!(
            search_config(&manifest(body), false).is_err(),
            "accepted: {body}"
        );
    }
}

#[test]
fn a_question_narrowed_to_names_keeps_those_and_refuses_one_nothing_carries() {
    let all = vec![
        Place::at("a".to_string(), "a.sqlite".into()),
        Place::at("b".to_string(), "b.sqlite".into()),
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
fn a_summarised_search_prints_the_summary_in_place_of_the_hits() {
    use homma_api::SummaryConfig;

    use crate::cmd::summary::OneShot;

    struct Echo;
    impl OneShot for Echo {
        fn ask(&self, system: &str, content: &str) -> Result<String, String> {
            assert!(system.contains("a search for \"saturation\""), "{system}");
            assert!(content.contains("one.md:"), "{content}");
            Ok("- one.md:3: the saturation of the air".to_string())
        }
    }

    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a.sqlite");
    index(&a, &[("one.md", "# One\n\nthe saturation of the air\n")]);
    let places = vec![Place::at("first".to_string(), a)];
    let render = |out: &SearchOut| {
        let mut text = Vec::new();
        out.render_human(&mut text).unwrap();
        String::from_utf8(text).unwrap()
    };

    let on = SummaryConfig {
        enabled: true,
        ..SummaryConfig::default()
    };
    let out = answer(
        &places,
        "saturation",
        10,
        Some((&on, &Echo)),
        SystemTime::now(),
    );
    let text = render(&out);
    assert!(
        text.starts_with("- one.md:3: the saturation of the air\n"),
        "{text}"
    );
    assert!(!text.contains("first  one.md:"), "{text}");
    assert!(text.contains("INDEXES"), "{text}");
    let json = serde_json::to_value(&out).unwrap();
    assert_eq!(json["summary"], "- one.md:3: the saturation of the air");
    assert!(json.get("summary_note").is_none());

    // THE CONTROL: with the pass off the hits are printed as before, and the reason under them.
    let off = answer(
        &places,
        "saturation",
        10,
        Some((&SummaryConfig::default(), &Echo)),
        SystemTime::now(),
    );
    let text = render(&off);
    assert!(text.contains("first  one.md:"), "{text}");
    assert!(text.contains("summary: off"), "{text}");
    // and without `--summarise` nothing is said of a summary at all
    let plain = answer::<Echo>(&places, "saturation", 10, None, SystemTime::now());
    let text = render(&plain);
    assert!(
        text.contains("first  one.md:") && !text.contains("summary"),
        "{text}"
    );
    let cli =
        crate::cli::Cli::try_parse_from(["homma-engine", "search", "--summarise", "word"]).unwrap();
    assert!(matches!(cli.command, crate::cli::Command::Search {
        summarise: true,
        ..
    }));
}

#[test]
fn every_hit_names_its_index_and_an_index_not_built_is_a_line_under_the_answer() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a.sqlite");
    let b = t.path().join("b.sqlite");
    index(&a, &[("one.md", "# One\n\nthe saturation of the air\n")]);
    index(&b, &[("two.md", "# Two\n\nsaturation curves\n")]);
    let places = vec![
        Place::at("first".to_string(), a),
        Place::at("gone".to_string(), t.path().join("gone.sqlite")),
        Place::at("second".to_string(), b),
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

#[test]
fn a_summarised_search_with_no_hits_makes_no_call() {
    use std::cell::Cell;

    use homma_api::SummaryConfig;

    use crate::cmd::summary::OneShot;

    struct Counted(Cell<usize>);
    impl OneShot for Counted {
        fn ask(&self, _: &str, _: &str) -> Result<String, String> {
            self.0.set(self.0.get() + 1);
            Ok("- one.md:3: the saturation of the air".to_string())
        }
    }

    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a.sqlite");
    index(&a, &[("one.md", "# One\n\nthe saturation of the air\n")]);
    let places = vec![Place::at("first".to_string(), a)];
    let on = SummaryConfig {
        enabled: true,
        ..SummaryConfig::default()
    };
    let model = Counted(Cell::new(0));
    let out = answer(
        &places,
        "zyzzogeton",
        10,
        Some((&on, &model)),
        SystemTime::now(),
    );
    assert!(out.hits.is_empty());
    assert_eq!(model.0.get(), 0);
    assert_eq!(out.summary, None);
    assert!(
        out.summary_note
            .as_deref()
            .is_some_and(|n| n.contains("no hits to summarise")),
        "{:?}",
        out.summary_note
    );

    // THE CONTROL: a search with a hit is sent.
    let out = answer(
        &places,
        "saturation",
        10,
        Some((&on, &model)),
        SystemTime::now(),
    );
    assert_eq!(model.0.get(), 1);
    assert!(out.summary.is_some());
}
