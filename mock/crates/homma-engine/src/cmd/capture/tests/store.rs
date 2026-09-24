//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::path::Path;

use jiff::Timestamp;

use crate::cmd::capture::store::{
    escaped,
    floor,
    mentioned,
    session_of,
    slug,
    transcript,
    watermark,
};

fn at(s: &str) -> Timestamp {
    s.parse().expect("an instant")
}

fn capture(source: Option<&str>, through: Option<&str>) -> String {
    let mut s = String::from("---\nwhen: 2026-09-24 17:00\nkind: unprompted\n");
    if let Some(v) = source {
        s.push_str(&format!("source: {v}\n"));
    }
    if let Some(v) = through {
        s.push_str(&format!("through: {v}\n"));
    }
    s.push_str(
        "tags: [chat]\n---\n\n# t\n\nsource: not-front-matter\nthrough: 2099-01-01T00:00:00Z\n",
    );
    s
}

#[test]
fn the_transcript_directory_is_the_path_with_every_other_character_a_dash() {
    assert_eq!(
        escaped(Path::new("/Users/me/Dev/clause-work/patchwork")),
        "-Users-me-Dev-clause-work-patchwork"
    );
    // Read off the harness's own directories: a dot and an underscore go too.
    assert_eq!(
        escaped(Path::new("/Users/me/.claude-worktrees/a_b")),
        "-Users-me--claude-worktrees-a-b"
    );
    assert_eq!(escaped(Path::new("/tmp/ä")), "-tmp--");
}

#[test]
fn the_watermark_is_the_latest_through_for_the_session_and_no_other() {
    let dir = tempfile::tempdir().expect("a directory");
    let d = dir.path();
    std::fs::write(
        d.join("a.md"),
        capture(Some("s1"), Some("2026-09-24T10:00:00Z")),
    )
    .unwrap();
    std::fs::write(
        d.join("b.md"),
        capture(Some("s1"), Some("2026-09-24T12:00:00Z")),
    )
    .unwrap();
    std::fs::write(
        d.join("c.md"),
        capture(Some("s2"), Some("2026-09-25T00:00:00Z")),
    )
    .unwrap();
    // A capture the archivist wrote names no session, and front-matter-looking
    // lines in a body are not front matter.
    std::fs::write(d.join("d.md"), capture(None, None)).unwrap();
    std::fs::write(
        d.join("e.txt"),
        capture(Some("s1"), Some("2027-01-01T00:00:00Z")),
    )
    .unwrap();
    std::fs::write(d.join("f.md"), "no front matter at all\n").unwrap();
    assert_eq!(
        watermark(d, "s1").unwrap(),
        Some(at("2026-09-24T12:00:00Z"))
    );
    assert_eq!(
        watermark(d, "s2").unwrap(),
        Some(at("2026-09-25T00:00:00Z"))
    );
    assert_eq!(watermark(d, "s3").unwrap(), None);
}

#[test]
fn a_store_that_is_not_there_yet_has_no_watermark() {
    let dir = tempfile::tempdir().expect("a directory");
    assert_eq!(watermark(&dir.path().join("absent"), "s").unwrap(), None);
}

#[test]
fn a_capture_of_the_session_with_a_bad_through_is_refused_by_name() {
    for bad in [None, Some("not a time")] {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(dir.path().join("x.md"), capture(Some("s"), bad)).unwrap();
        let err = format!("{:#}", watermark(dir.path(), "s").expect_err("refused"));
        assert!(err.contains("x.md"), "{err}");
        // The same file under another session is none of this run's business.
        assert_eq!(watermark(dir.path(), "other").unwrap(), None);
    }
}

#[test]
fn the_later_floor_wins_and_either_alone_stands() {
    let (a, b) = (at("2026-09-24T10:00:00Z"), at("2026-09-24T11:00:00Z"));
    assert_eq!(floor(Some(a), Some(b)), Some(b));
    assert_eq!(floor(Some(b), Some(a)), Some(b));
    assert_eq!(floor(Some(a), None), Some(a));
    assert_eq!(floor(None, Some(a)), Some(a));
    assert_eq!(floor(None, None), None);
}

#[test]
fn a_slug_keeps_letters_and_digits_and_one_dash_between() {
    assert_eq!(
        slug("Pausing goal polling, when blocked!").unwrap(),
        "pausing-goal-polling-when-blocked"
    );
    assert_eq!(
        slug("  --Lifebook's 2nd night--  ").unwrap(),
        "lifebook-s-2nd-night"
    );
    assert_eq!(slug("ÄÖ mix äö").unwrap(), "mix");
    for nothing in ["", "   ", "!!!", "äö"] {
        assert!(slug(nothing).is_err(), "{nothing:?}");
    }
}

#[test]
fn a_long_slug_is_cut_at_a_word_and_never_past_sixty() {
    let long = "word ".repeat(40);
    let s = slug(&long).unwrap();
    assert!(s.len() <= 60, "{s}");
    assert!(s.ends_with("word") && !s.ends_with('-'), "{s}");
    // One word longer than the limit has no boundary and is cut at it.
    let one = "x".repeat(80);
    assert_eq!(slug(&one).unwrap().len(), 60);
}

#[test]
fn a_session_names_a_file_an_id_or_the_newest() {
    let dir = tempfile::tempdir().expect("a directory");
    let d = dir.path();
    std::fs::write(d.join("old.jsonl"), "").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(d.join("new.jsonl"), "").unwrap();
    std::fs::write(d.join("newer.txt"), "").unwrap();
    assert_eq!(transcript(d, None).unwrap(), d.join("new.jsonl"));
    assert_eq!(transcript(d, Some("old")).unwrap(), d.join("old.jsonl"));
    let by_path = d.join("old.jsonl");
    assert_eq!(
        transcript(Path::new("/nowhere"), Some(by_path.to_str().unwrap())).unwrap(),
        by_path
    );
    let err = format!("{:#}", transcript(d, Some("absent")).expect_err("no such"));
    assert!(err.contains("absent"), "{err}");
    assert_eq!(session_of(&d.join("new.jsonl")).unwrap(), "new");
}

#[test]
fn no_transcripts_at_all_is_refused_with_what_to_do() {
    let dir = tempfile::tempdir().expect("a directory");
    let empty = format!("{:#}", transcript(dir.path(), None).expect_err("empty"));
    assert!(empty.contains("--session"), "{empty}");
    let absent = format!(
        "{:#}",
        transcript(&dir.path().join("x"), None).expect_err("absent")
    );
    assert!(absent.contains("--session"), "{absent}");
}

#[test]
fn a_mentioned_path_counts_only_where_it_exists_under_the_root() {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path();
    std::fs::create_dir_all(root.join(".shared/state")).unwrap();
    std::fs::write(root.join(".shared/state/arc.md"), "").unwrap();
    std::fs::write(root.join("README.md"), "").unwrap();
    let outside = tempfile::tempdir().expect("another");
    std::fs::write(outside.path().join("x.md"), "").unwrap();
    let abs = root.join("README.md");
    let words = format!(
        "see `.shared/state/arc.md`, and (README.md). also {} and {}/x.md and \
         nothere/at.all and ../escape.md and .shared/state/ again .shared/state/arc.md",
        abs.display(),
        outside.path().display()
    );
    assert_eq!(mentioned(root, &[&words]), [
        ".shared/state/arc.md",
        "README.md",
        ".shared/state",
    ]);
    // Words that name no path give none.
    assert!(mentioned(root, &["plain words only"]).is_empty());
}
