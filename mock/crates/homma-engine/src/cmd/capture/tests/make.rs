//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The whole path from transcript to file text, and the watermark over a real
//! store directory holding what an earlier run wrote.

use std::path::Path;

use jiff::tz::TimeZone;

use super::*;
use crate::cmd::capture::transcript::read;
use crate::cmd::capture::{Ask, Made, make, store};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:05:00.000Z";
const T2: &str = "2026-09-24T14:10:00.000Z";

fn ask<'a>(bears_on: &'a [String]) -> Ask<'a> {
    Ask {
        title: "What he said",
        session: None,
        since: None,
        bears_on,
        into: None,
    }
}

/// One run as `run` makes it, against the store in `store_dir`, writing what
/// it made there so the next run sees it.
fn run_over(store_dir: &Path, text: &str, a: &Ask<'_>) -> Option<Made> {
    let t = read(text).unwrap();
    let mark = store::watermark(store_dir, "s", &t).unwrap();
    let made = make(Path::new("/"), &t, "s", mark.as_ref(), a, &TimeZone::UTC).unwrap();
    if let Some(m) = &made {
        std::fs::write(store_dir.join(&m.name), &m.body).unwrap();
    }
    made
}

#[test]
fn a_second_run_over_the_same_transcript_writes_nothing() {
    let dir = tempfile::tempdir().expect("a directory");
    let text = lines(&[typed("a", T0, "first"), typed("b", T1, "second")]);
    let first = run_over(dir.path(), &text, &ask(&[])).expect("something new");
    assert_eq!((first.said, first.asked), (2, 0));
    assert!(first.body.contains("\nthrough: b\n"), "{}", first.body);
    assert!(
        run_over(dir.path(), &text, &ask(&[])).is_none(),
        "the same transcript again"
    );

    // The session goes on; the next run takes only what came after.
    let more = format!("{text}{}", lines(&[round("r", T2, "Left", "Red", None)]));
    let second = run_over(dir.path(), &more, &ask(&[])).expect("the new round");
    assert_eq!((second.said, second.asked), (0, 1));
    assert!(
        !second.body.contains("first") && !second.body.contains("> second"),
        "{}",
        second.body
    );
    assert!(second.body.contains("\nthrough: r\n"), "{}", second.body);
    assert_eq!(second.name, "202609241410_what-he-said.md");
}

#[test]
fn a_message_queued_during_a_turn_is_not_lost_to_the_next_run() {
    // What the review found against a timestamp watermark, as the harness
    // writes it: an ask round answered at T2, the capture taken right after,
    // then a message he typed at T1 while the turn ran, written after it.
    let dir = tempfile::tempdir().expect("a directory");
    let before = lines(&[typed("a", T0, "go"), round("r", T2, "Left", "Red", None)]);
    let first = run_over(dir.path(), &before, &ask(&[])).expect("the round");
    assert_eq!((first.said, first.asked), (1, 1));
    let after = format!(
        "{before}{}",
        lines(&[queued("q", T1, "typed while you worked")])
    );
    let second = run_over(dir.path(), &after, &ask(&[])).expect("the queued message");
    assert_eq!((second.said, second.asked), (1, 0));
    assert!(
        second.body.contains("> typed while you worked"),
        "{}",
        second.body
    );
    // Named for when it was said, which is earlier than the capture before it.
    assert_eq!(second.name, "202609241405_what-he-said.md");
}

#[test]
fn a_twin_split_across_two_runs_is_read_once() {
    let dir = tempfile::tempdir().expect("a directory");
    let mut q = queued("q", T0, "while you work");
    q["attachment"]["source_uuid"] = serde_json::json!("t");
    let before = lines(&[q]);
    assert!(run_over(dir.path(), &before, &ask(&[])).is_some());
    // The typed twin lands after the watermark; it is the same words.
    let after = format!("{before}{}", lines(&[typed("t", T1, "while you work")]));
    assert!(run_over(dir.path(), &after, &ask(&[])).is_none());
}

#[test]
fn a_floor_by_hand_takes_only_what_is_stamped_after_it() {
    let text = lines(&[typed("a", T0, "before"), typed("b", T1, "at"), typed("c", T2, "after")]);
    let t = read(&text).unwrap();
    let a = Ask {
        since: Some(T1.parse().unwrap()),
        ..ask(&[])
    };
    let m = make(Path::new("/"), &t, "s", None, &a, &TimeZone::UTC)
        .unwrap()
        .expect("one event");
    assert_eq!(m.said, 1);
    assert!(
        m.body.contains("> after") && !m.body.contains("> at\n"),
        "{}",
        m.body
    );
    // A floor past everything writes nothing.
    let late = Ask {
        since: Some("2027-01-01T00:00:00Z".parse().unwrap()),
        ..ask(&[])
    };
    assert!(
        make(Path::new("/"), &t, "s", None, &late, &TimeZone::UTC)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_floor_by_hand_and_a_watermark_both_hold() {
    // Past the watermark by line, and stamped after `--since`: `c` is past the
    // mark and too early, `d` is both.
    let text = lines(&[typed("a", T0, "marked"), typed("c", T0, "early"), typed("d", T2, "late")]);
    let t = read(&text).unwrap();
    let mark = store::Mark {
        line: 0,
        uuid: "a".into(),
    };
    let a = Ask {
        since: Some(T1.parse().unwrap()),
        ..ask(&[])
    };
    let m = make(Path::new("/"), &t, "s", Some(&mark), &a, &TimeZone::UTC)
        .unwrap()
        .expect("one");
    assert_eq!(m.said, 1);
    assert!(
        m.body.contains("> late") && !m.body.contains("> early"),
        "{}",
        m.body
    );
}

#[test]
fn what_it_bears_on_is_what_was_named_then_what_was_mentioned() {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path();
    std::fs::write(root.join("GOAL.md"), "").unwrap();
    std::fs::write(root.join("named.md"), "").unwrap();
    let t = read(&lines(&[typed(
        "a",
        T0,
        "see GOAL.md and named.md and missing.md",
    )]))
    .unwrap();
    let named = ["muisti".to_string(), "named.md".to_string()];
    let m = make(root, &t, "s", None, &ask(&named), &TimeZone::UTC)
        .unwrap()
        .expect("made");
    assert!(
        m.body
            .contains("bears_on:\n  - muisti\n  - named.md\n  - GOAL.md\ntags"),
        "{}",
        m.body
    );
}

#[test]
fn a_title_with_nothing_to_name_a_file_by_is_refused() {
    let t = read(&lines(&[typed("a", T0, "x")])).unwrap();
    let a = Ask {
        title: "!!!",
        ..ask(&[])
    };
    let err = format!(
        "{:#}",
        make(Path::new("/"), &t, "s", None, &a, &TimeZone::UTC).expect_err("refused")
    );
    assert!(err.contains("no letter or digit"), "{err}");
}

#[test]
fn events_with_no_uuid_anywhere_have_nothing_to_mark_the_capture_by() {
    let mut line = typed("a", T0, "x");
    line.as_object_mut().unwrap().remove("uuid");
    let t = read(&lines(&[line])).unwrap();
    let err = format!(
        "{:#}",
        make(Path::new("/"), &t, "s", None, &ask(&[]), &TimeZone::UTC).expect_err("refused")
    );
    assert!(err.contains("`uuid`"), "{err}");
}
