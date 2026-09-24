//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::path::Path;

use super::*;
use crate::cmd::capture::store::{
    Mark,
    escaped,
    mentioned,
    session_of,
    slug,
    transcript,
    watermark,
};
use crate::cmd::capture::transcript::{Transcript, read};

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

fn mark(line: usize, uuid: &str) -> Option<Mark> {
    Some(Mark {
        line,
        uuid: uuid.to_string(),
    })
}

#[test]
fn the_watermark_is_the_through_furthest_into_the_transcript_and_no_other_sessions() {
    // Stamped backwards, as a queued message is: `late` is written first and
    // carries the later time. The file order is what counts.
    let t = read(&lines(&[
        typed("late", "2026-09-24T12:00:00Z", "x"),
        typed("mid", "2026-09-24T11:00:00Z", "y"),
        typed("early", "2026-09-24T10:00:00Z", "z"),
    ]))
    .expect("reads");
    let dir = tempfile::tempdir().expect("a directory");
    let d = dir.path();
    std::fs::write(d.join("a.md"), capture(Some("s1"), Some("late"))).unwrap();
    std::fs::write(d.join("b.md"), capture(Some("s1"), Some("early"))).unwrap();
    std::fs::write(d.join("c.md"), capture(Some("s1"), Some("mid"))).unwrap();
    std::fs::write(d.join("z.md"), capture(Some("s2"), Some("mid"))).unwrap();
    // A capture the archivist wrote names no session, and front-matter-looking
    // lines in a body are not front matter.
    std::fs::write(d.join("d.md"), capture(None, None)).unwrap();
    std::fs::write(d.join("e.txt"), capture(Some("s1"), Some("nowhere"))).unwrap();
    std::fs::write(d.join("f.md"), "no front matter at all\n").unwrap();
    assert_eq!(watermark(d, "s1", &t).unwrap(), mark(2, "early"));
    assert_eq!(watermark(d, "s2", &t).unwrap(), mark(1, "mid"));
    assert_eq!(watermark(d, "s3", &t).unwrap(), None);
}

#[test]
fn a_store_that_is_not_there_yet_has_no_watermark() {
    let dir = tempfile::tempdir().expect("a directory");
    let t = Transcript::default();
    assert_eq!(
        watermark(&dir.path().join("absent"), "s", &t).unwrap(),
        None
    );
}

#[test]
fn a_capture_of_the_session_with_a_through_the_transcript_lacks_is_refused_by_name() {
    let t = read(&lines(&[typed("a", "2026-09-24T10:00:00Z", "x")])).expect("reads");
    // No `through`, a time where a uuid belongs, and a uuid of some other
    // transcript or of a rewritten one.
    for bad in [None, Some("2026-09-24T10:00:00Z"), Some("b")] {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(dir.path().join("x.md"), capture(Some("s"), bad)).unwrap();
        let err = format!("{:#}", watermark(dir.path(), "s", &t).expect_err("refused"));
        assert!(err.contains("x.md"), "{err}");
        // The same file under another session is none of this run's business.
        assert_eq!(watermark(dir.path(), "other", &t).unwrap(), None);
    }
    // The control: the same file naming a line the transcript has.
    let dir = tempfile::tempdir().expect("a directory");
    std::fs::write(dir.path().join("x.md"), capture(Some("s"), Some("a"))).unwrap();
    assert_eq!(watermark(dir.path(), "s", &t).unwrap(), mark(0, "a"));
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
    // Times set rather than waited for, so a filesystem with coarse stamps
    // cannot tie them; the newest file of all is not a transcript.
    let at = |name: &str, secs: u64| {
        let f = std::fs::File::create(d.join(name)).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    at("new.jsonl", 2_000_000_000);
    at("old.jsonl", 1_000_000_000);
    at("newer.txt", 2_100_000_000);
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
