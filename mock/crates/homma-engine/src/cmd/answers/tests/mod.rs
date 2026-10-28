//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The answers command, one behaviour per test. The golden files are what the dashboard's drain
//! wrote for the same four documents, so the capture is compared byte for byte.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::docs::{self, Collection};
use super::{Run, Source, attach, delegate, hook, render, take_documents};

const CAPTURE: &str = include_str!("golden/capture.golden");
const TITLE: &str = include_str!("golden/title.txt");

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cmd/answers/tests/golden")
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn answer(id: &str, key: serde_json::Value, kind: &str, at: &str) -> String {
    json!({"id": id, "key": key, "kind": kind, "where": "kaski", "asks": "Which?",
           "options": ["a", "b"], "choice": "b", "text": "because", "at": at})
    .to_string()
}

fn run_in(tmp: &Path, mock: Option<PathBuf>) -> Run {
    fs::create_dir_all(tmp.join("kaski/.git")).unwrap();
    Run {
        root: tmp.to_path_buf(),
        store: tmp.join("store"),
        work: Some("kaski".into()),
        mock,
    }
}

// ---- docs: one refusal each

fn refused(collection: Collection, body: serde_json::Value) -> String {
    docs::parse(collection, &body.to_string()).expect_err("must be refused")
}

#[test]
fn a_timestamp_that_is_not_one_is_refused() {
    let why = refused(Collection::Notes, json!({"text": "x", "at": "yesterday"}));
    assert!(why.contains("`at`"), "{why}");
}

#[test]
fn a_text_that_is_not_text_is_refused() {
    let why = refused(
        Collection::Notes,
        json!({"text": 4, "at": "2026-10-28T09:40:00Z"}),
    );
    assert!(why.contains("`text`"), "{why}");
}

#[test]
fn an_id_with_a_slash_is_refused() {
    let doc = answer("../x", json!("q"), "registry", "2026-10-28T09:40:00Z");
    let why = docs::parse(Collection::Answers, &doc).unwrap_err();
    assert!(why.contains("`id`"), "{why}");
}

#[test]
fn a_key_of_two_pairs_is_refused() {
    let doc = answer(
        "q",
        json!({"a": "1", "b": "2"}),
        "ticket",
        "2026-10-28T09:40:00Z",
    );
    let why = docs::parse(Collection::Answers, &doc).unwrap_err();
    assert!(why.contains("`key`"), "{why}");
}

#[test]
fn a_where_with_a_path_in_it_is_refused() {
    let mut v: serde_json::Value =
        serde_json::from_str(&answer("q", json!("q"), "registry", "2026-10-28T09:40:00Z")).unwrap();
    v["where"] = json!("../etc");
    assert!(
        docs::parse(Collection::Answers, &v.to_string())
            .unwrap_err()
            .contains("`where`")
    );
}

#[test]
fn an_option_with_a_control_character_is_refused() {
    let mut v: serde_json::Value =
        serde_json::from_str(&answer("q", json!("q"), "registry", "2026-10-28T09:40:00Z")).unwrap();
    v["options"] = json!(["a\u{1b}[31m"]);
    assert!(
        docs::parse(Collection::Answers, &v.to_string())
            .unwrap_err()
            .contains("options")
    );
}

#[test]
fn a_console_ticket_document_is_taken() {
    let doc = answer(
        "tk",
        json!({"ticket": "tk_one"}),
        "ticket",
        "2026-10-28T09:40:00Z",
    );
    let d = docs::parse(Collection::Answers, &doc).unwrap();
    assert_eq!(d.label(), "ticket::tk_one");
    assert_eq!(d.key, "answers/tk@2026-10-28T09:40:00Z");
}

#[test]
fn a_directory_read_names_the_skipped_and_takes_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("notes")).unwrap();
    fs::write(
        tmp.path().join("notes/ok.json"),
        r#"{"text":"t","at":"2026-10-28T09:40:00Z"}"#,
    )
    .unwrap();
    fs::write(tmp.path().join("notes/bad.json"), "{").unwrap();
    let read = docs::read_dir(tmp.path()).unwrap();
    assert_eq!(read.taken.len(), 1);
    assert_eq!(read.skipped.len(), 1);
    assert!(read.skipped[0].contains("bad.json"));
}

#[test]
fn a_channel_read_takes_answers_and_notes_and_skips_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let body_a = answer("q", json!("q"), "registry", "2026-10-28T09:10:00Z");
    let listing = json!({"messages": [
        {"id": "m1", "kind": "answer", "body": body_a},
        {"id": "m2", "kind": "note", "body": r#"{"text":"t","at":"2026-10-28T09:40:00Z"}"#},
        {"id": "m3", "kind": "status", "body": "working"},
        {"id": "m4", "kind": "answer", "body": "not json"},
    ]});
    fs::write(tmp.path().join("listing.json"), listing.to_string()).unwrap();
    let msg = script(
        tmp.path(),
        "msg",
        &format!("cat '{}'", tmp.path().join("listing.json").display()),
    );
    let read = docs::read_channel(tmp.path(), &msg, "kaski", "op").unwrap();
    assert_eq!(read.taken.len(), 2);
    assert_eq!(read.skipped.len(), 1);
    assert!(read.skipped[0].contains("m4"));
}

#[test]
fn a_channel_with_nothing_from_op_reads_as_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let msg = script(tmp.path(), "msg", "echo 'nothing from op' >&2; exit 1");
    let read = docs::read_channel(tmp.path(), &msg, "kaski", "op").unwrap();
    assert!(read.taken.is_empty());
}

#[test]
fn a_channel_that_fails_for_another_reason_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let msg = script(tmp.path(), "msg", "echo 'broken' >&2; exit 1");
    assert!(docs::read_channel(tmp.path(), &msg, "kaski", "op").is_err());
}

// ---- render

fn golden_docs() -> Vec<docs::Document> {
    let g = golden();
    let mut read = docs::read_dir(&g).unwrap();
    assert!(read.skipped.is_empty(), "{:?}", read.skipped);
    read.taken.sort_by(|a, b| a.at.cmp(&b.at));
    read.taken
}

#[test]
fn the_title_is_the_drains() {
    assert_eq!(
        render::title(&golden_docs(), &Source::Dir(Path::new("fixture"))),
        TITLE.trim_end()
    );
}

#[test]
fn the_capture_is_the_drains_byte_for_byte() {
    let d = golden_docs();
    let source = Source::Dir(Path::new("fixture"));
    let title = render::title(&d, &source);
    let got = render::text(&d, "2026-10-28 09:40", &source, &title);
    assert_eq!(got, CAPTURE);
}

#[test]
fn a_fence_inside_the_question_gets_a_longer_fence() {
    let mut v: serde_json::Value =
        serde_json::from_str(&answer("q", json!("q"), "registry", "2026-10-28T09:10:00Z")).unwrap();
    v["asks"] = json!("run ```x``` now");
    let d = docs::parse(Collection::Answers, &v.to_string()).unwrap();
    let got = render::text(&[d], "w", &Source::Dir(Path::new("s")), "t");
    assert!(got.contains("  ````text\n"), "{got}");
}

#[test]
fn the_name_is_the_minute_and_the_slug_and_a_taken_name_gets_the_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let d = golden_docs();
    let first = render::name(tmp.path(), "2026-10-28T09:40:00Z", "op answers q1", &d);
    assert_eq!(first, "202610280940_op-answers-q1.md");
    fs::write(tmp.path().join(&first), "x").unwrap();
    let second = render::name(tmp.path(), "2026-10-28T09:40:00Z", "op answers q1", &d);
    assert_ne!(first, second);
    assert!(
        second.starts_with("202610280940_op-answers-q1-"),
        "{second}"
    );
}

#[test]
fn recorded_reads_the_front_matter_only() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("c.md"),
        "---\nstore:\n  - answers/q1@x\n---\n\n  - answers/not-a-key@y\n",
    )
    .unwrap();
    let keys = render::recorded(tmp.path()).unwrap();
    assert!(keys.contains("answers/q1@x"));
    assert!(!keys.contains("answers/not-a-key@y"));
}

// ---- attach

#[test]
fn an_object_key_names_its_own_row() {
    let doc = answer(
        "tk",
        json!({"ticket": "tk_one"}),
        "ticket",
        "2026-10-28T09:40:00Z",
    );
    let d = docs::parse(Collection::Answers, &doc).unwrap();
    assert_eq!(attach::target(&d).unwrap().spelled(), "ticket::tk_one");
}

#[test]
fn a_registry_question_names_a_question_row_and_an_agenda_one_names_none() {
    let reg = docs::parse(
        Collection::Answers,
        &answer("q", json!("q9"), "registry", "2026-10-28T09:10:00Z"),
    )
    .unwrap();
    assert_eq!(attach::target(&reg).unwrap().spelled(), "question::q9");
    let ag = docs::parse(
        Collection::Answers,
        &answer("q", json!("q9"), "agenda", "2026-10-28T09:10:00Z"),
    )
    .unwrap();
    assert!(attach::target(&ag).is_none());
}

#[test]
fn the_row_is_written_through_the_launcher_with_local() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("args");
    let mock = script(
        tmp.path(),
        "mock",
        &format!("echo \"$@\" > '{}'", log.display()),
    );
    let t = attach::Target {
        namespace: "ticket".into(),
        id:        "tk_one".into(),
    };
    attach::row(Some(&mock), tmp.path(), &t, "2026-10-28T09:40:00Z", "p.md").unwrap();
    assert_eq!(
        fs::read_to_string(log).unwrap().trim(),
        "registry-append ticket::tk_one answers 2026-10-28T09:40:00Z: p.md --local"
    );
}

#[test]
fn a_refused_row_is_reported_with_the_last_line_of_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = script(
        tmp.path(),
        "mock",
        "echo 'first' >&2; echo 'no such row' >&2; exit 2",
    );
    let t = attach::Target {
        namespace: "ticket".into(),
        id:        "x".into(),
    };
    let e = attach::row(Some(&mock), tmp.path(), &t, "a", "b").unwrap_err();
    assert!(e.contains("no such row") && e.contains("ticket::x"), "{e}");
}

// ---- take

#[test]
fn a_take_writes_one_capture_attaches_and_a_second_take_does_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("args");
    let mock = script(
        tmp.path(),
        "mock",
        &format!("echo \"$@\" >> '{}'", log.display()),
    );
    let run = run_in(tmp.path(), Some(mock));
    let doc = |d: &str| {
        docs::parse(
            Collection::Answers,
            &answer("tk", json!({"ticket": "tk_one"}), "ticket", d),
        )
        .unwrap()
    };
    let read = || {
        docs::Read {
            taken:   vec![doc("2026-10-28T09:40:00Z")],
            skipped: vec![],
        }
    };
    let first = take_documents(&run, read(), &Source::Dir(tmp.path()), None).unwrap();
    assert_eq!(first.answers, 1);
    assert_eq!(first.attached, vec!["ticket::tk_one".to_string()]);
    assert_eq!(fs::read_dir(&run.store).unwrap().count(), 1);
    let second = take_documents(&run, read(), &Source::Dir(tmp.path()), None).unwrap();
    assert!(second.written.is_none());
    assert_eq!(fs::read_dir(&run.store).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 1);
}

#[test]
fn a_row_that_cannot_be_written_leaves_the_capture_and_is_named() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = script(tmp.path(), "mock", "echo 'nope' >&2; exit 1");
    let run = run_in(tmp.path(), Some(mock));
    let d = docs::parse(
        Collection::Answers,
        &answer(
            "tk",
            json!({"ticket": "t"}),
            "ticket",
            "2026-10-28T09:40:00Z",
        ),
    )
    .unwrap();
    let taken = take_documents(
        &run,
        docs::Read {
            taken:   vec![d],
            skipped: vec![],
        },
        &Source::Channel {
            channel: "kaski",
            from:    "op",
        },
        None,
    )
    .unwrap();
    assert!(taken.written.is_some());
    assert!(taken.attached.is_empty());
    assert_eq!(taken.left.len(), 1);
}

// ---- delegate

#[test]
fn the_delegate_takes_words_and_declines_everything_else() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_in(tmp.path(), Some(script(tmp.path(), "mock", "exit 0")));
    let repo = tmp.path().join("kaski");
    let done = delegate::run(
        &run,
        &repo,
        "ticket::t1",
        "answers",
        "2026-10-28T09:40:00Z: yes, do it",
    )
    .unwrap();
    assert_eq!(done, delegate::Delegated::Whole);
    assert_eq!(fs::read_dir(&run.store).unwrap().count(), 1);
    let other_field = delegate::run(
        &run,
        &repo,
        "ticket::t1",
        "title",
        "2026-10-28T09:40:00Z: x",
    )
    .unwrap();
    assert_eq!(other_field, delegate::Delegated::Declined);
    let not_words = delegate::run(
        &run,
        &repo,
        "ticket::t1",
        "answers",
        "2026-10-28T09:40:00Z: ",
    )
    .unwrap();
    assert_eq!(not_words, delegate::Delegated::Declined);
    let pointer = delegate::run(&run, &repo, "ticket::t1", "answers", "nonsense").unwrap();
    assert_eq!(pointer, delegate::Delegated::Declined);
}

// ---- hook

#[test]
fn the_hook_is_placed_executable_and_removed() {
    let tmp = tempfile::tempdir().unwrap();
    hook::place(tmp.path(), Path::new("/bin/homma")).unwrap();
    let p = hook::path(tmp.path());
    let text = fs::read_to_string(&p).unwrap();
    assert!(text.starts_with("#!/bin/sh\n"));
    assert!(text.contains("answers delegate"), "{text}");
    assert_ne!(fs::metadata(&p).unwrap().permissions().mode() & 0o111, 0);
    hook::remove(tmp.path()).unwrap();
    assert!(!p.exists());
    hook::remove(tmp.path()).unwrap();
}

// ---- the review's findings

#[test]
fn a_kind_that_is_not_a_plain_name_is_refused_and_a_console_kind_is_taken() {
    let mut v: serde_json::Value = serde_json::from_str(&answer(
        "q",
        json!({"ticket": "t"}),
        "ticket",
        "2026-10-28T09:40:00Z",
    ))
    .unwrap();
    assert!(docs::parse(Collection::Answers, &v.to_string()).is_ok());
    for bad in ["../x", "a b", "", "x/y"] {
        v["kind"] = json!(bad);
        let why = docs::parse(Collection::Answers, &v.to_string()).unwrap_err();
        assert!(why.contains("`kind`"), "{bad:?}: {why}");
    }
    v["kind"] = json!(7);
    assert!(
        docs::parse(Collection::Answers, &v.to_string())
            .unwrap_err()
            .contains("`kind`")
    );
}

#[test]
fn a_control_character_in_a_choice_is_refused() {
    let mut v: serde_json::Value =
        serde_json::from_str(&answer("q", json!("q"), "registry", "2026-10-28T09:40:00Z")).unwrap();
    v["choice"] = json!("b\u{1b}[31m");
    assert!(
        docs::parse(Collection::Answers, &v.to_string())
            .unwrap_err()
            .contains("`choice`")
    );
    v["choice"] = json!(["a", "b\u{7}"]);
    assert!(
        docs::parse(Collection::Answers, &v.to_string())
            .unwrap_err()
            .contains("`choice`")
    );
}

#[test]
fn a_delegate_whose_row_cannot_be_written_fails_and_says_why() {
    let tmp = tempfile::tempdir().unwrap();
    let mock = script(tmp.path(), "mock", "echo 'no such row' >&2; exit 2");
    let run = run_in(tmp.path(), Some(mock));
    let repo = tmp.path().join("kaski");
    let done = delegate::run(
        &run,
        &repo,
        "ticket::t1",
        "answers",
        "2026-10-28T09:40:00Z: yes",
    )
    .unwrap();
    let delegate::Delegated::Failed(why) = done else { panic!("{done:?}") };
    assert!(
        why.contains("no such row") && why.contains("ticket::t1"),
        "{why}"
    );
    assert_eq!(
        fs::read_dir(&run.store).unwrap().count(),
        0,
        "the capture is taken away again"
    );
}

#[test]
fn a_delegate_attaches_with_local_and_the_capture_path_it_wrote() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("args");
    let mock = script(
        tmp.path(),
        "mock",
        &format!("echo \"$@\" >> '{}'", log.display()),
    );
    let run = run_in(tmp.path(), Some(mock));
    let repo = tmp.path().join("kaski");
    delegate::run(
        &run,
        &repo,
        "ticket::t1",
        "answers",
        "2026-10-28T09:40:00Z: yes",
    )
    .unwrap();
    let args = fs::read_to_string(log).unwrap();
    assert!(
        args.starts_with("registry-append ticket::t1 answers 2026-10-28T09:40:00Z: ")
            && args.trim_end().ends_with(".md --local"),
        "{args}"
    );
}

#[test]
fn two_appends_at_one_time_with_different_words_are_two_captures_and_the_same_words_are_one() {
    let tmp = tempfile::tempdir().unwrap();
    let run = run_in(tmp.path(), Some(script(tmp.path(), "mock", "exit 0")));
    let repo = tmp.path().join("kaski");
    let at = "2026-10-28T09:40:00Z";
    for words in ["yes", "no", "yes"] {
        delegate::run(
            &run,
            &repo,
            "ticket::t1",
            "answers",
            &format!("{at}: {words}"),
        )
        .unwrap();
    }
    let said: Vec<String> = fs::read_dir(&run.store)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said.iter().any(|c| c.contains("> yes")));
    assert!(said.iter().any(|c| c.contains("> no")));
}

#[test]
fn the_placed_hook_declines_when_its_engine_is_gone_and_runs_it_when_it_is_there() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    let engine = tmp.path().join("homma-engine");
    // Gone: the placed file exits 3, which leaves the write to mockspace.
    let file = hook::place(&ws, &engine).unwrap();
    let gone = std::process::Command::new(&file)
        .args(["ticket::t", "answers", "x"])
        .status()
        .unwrap();
    assert_eq!(gone.code(), Some(3));
    // There: it runs the engine named, with the workspace and the verb and the arguments given.
    let log = tmp.path().join("args");
    let engine = script(
        tmp.path(),
        "homma-engine",
        &format!("echo \"$@\" > '{}'; exit 0", log.display()),
    );
    let file = hook::place(&ws, &engine).unwrap();
    let ran = std::process::Command::new(&file)
        .args(["ticket::t", "answers", "x"])
        .status()
        .unwrap();
    assert_eq!(ran.code(), Some(0));
    assert_eq!(
        fs::read_to_string(log).unwrap().trim(),
        format!(
            "--dir {} answers delegate ticket::t answers x",
            ws.display()
        )
    );
    // Replaced: placing again names the new engine and no trace of the old.
    let other = script(tmp.path(), "other-engine", "exit 0");
    hook::place(&ws, &other).unwrap();
    let text = fs::read_to_string(hook::path(&ws)).unwrap();
    assert!(text.contains(other.to_str().unwrap()), "{text}");
    assert!(!text.contains(engine.to_str().unwrap()), "{text}");
}

#[test]
fn a_capture_says_where_its_words_were_left_and_who_left_them() {
    let docs = vec![
        docs::parse(
            Collection::Answers,
            &answer(
                "tk",
                json!({"ticket": "t1"}),
                "ticket",
                "2026-10-28T09:40:00Z",
            ),
        )
        .unwrap(),
    ];
    let note = docs::parse(
        Collection::Notes,
        r#"{"text":"t","at":"2026-10-28T09:41:00Z"}"#,
    )
    .unwrap();
    let from_the_page = Source::Dir(Path::new("fixture"));
    let from_the_channel = Source::Channel {
        channel: "kaski",
        from:    "coord",
    };
    let delegated = Source::Delegated("ticket::t1");

    let title = render::title(&docs, &from_the_channel);
    assert_eq!(title, "coord answers ticket::t1");
    let text = render::text(&docs, "2026-10-28 09:40", &from_the_channel, &title);
    assert!(text.contains("tags: [channel, answers]"), "{text}");
    assert!(text.contains("Left on the channel kaski,"), "{text}");
    assert!(!text.contains("dashboard"), "{text}");

    let text = render::text(&docs, "w", &delegated, &render::title(&docs, &delegated));
    assert!(text.contains("tags: [registry-append, answers]"), "{text}");
    assert!(text.contains("# someone answers ticket::t1"), "{text}");
    assert!(!text.contains("dashboard"), "{text}");

    // The controls: the page keeps its tag, its lead and `op`, and a note says where it was left.
    let text = render::text(
        &docs,
        "w",
        &from_the_page,
        &render::title(&docs, &from_the_page),
    );
    assert!(
        text.contains("tags: [dashboard, answers]") && text.contains("Left on the dashboard page")
    );
    assert!(text.contains("# op answers ticket::t1"), "{text}");
    assert_eq!(
        render::title(&[note.clone()], &from_the_page),
        "op leaves a note on the page"
    );
    assert_eq!(
        render::title(&[note], &from_the_channel),
        "coord leaves a note on the channel"
    );
}
