//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The whole path from transcript to file text, and the watermark over a real
//! store directory holding what an earlier run wrote.

use std::path::Path;

use jiff::tz::TimeZone;

use super::*;
use crate::cmd::capture::{Ask, make, store};

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

#[test]
fn a_second_run_over_the_same_transcript_writes_nothing() {
    let dir = tempfile::tempdir().expect("a directory");
    let store_dir = dir.path();
    let text = lines(&[typed("a", T0, "first"), typed("b", T1, "second")]);
    let run = |text: &str| {
        let floor = store::floor(store::watermark(store_dir, "s").unwrap(), None);
        make(Path::new("/"), text, "s", floor, &ask(&[]), &TimeZone::UTC).unwrap()
    };
    let first = run(&text).expect("something new");
    assert_eq!((first.said, first.asked), (2, 0));
    std::fs::write(store_dir.join(&first.name), &first.body).unwrap();
    assert!(run(&text).is_none(), "the same transcript again");

    // The session goes on; the next run takes only what came after.
    let more = format!("{text}{}", lines(&[round("r", T2, "Left", "Red", None)]));
    let second = run(&more).expect("the new round");
    assert_eq!((second.said, second.asked), (0, 1));
    assert!(
        !second.body.contains("first") && !second.body.contains("> second"),
        "{}",
        second.body
    );
    assert_eq!(second.name, "202609241410_what-he-said.md");
}

#[test]
fn a_floor_by_hand_takes_only_what_came_after_it() {
    let text = lines(&[typed("a", T0, "before"), typed("b", T1, "at"), typed("c", T2, "after")]);
    let floor = Some(T1.parse().unwrap());
    let m = make(Path::new("/"), &text, "s", floor, &ask(&[]), &TimeZone::UTC)
        .unwrap()
        .expect("one event");
    assert_eq!(m.said, 1);
    assert!(
        m.body.contains("> after") && !m.body.contains("> at\n"),
        "{}",
        m.body
    );
    // A floor past everything writes nothing.
    let late = Some("2027-01-01T00:00:00Z".parse().unwrap());
    assert!(
        make(Path::new("/"), &text, "s", late, &ask(&[]), &TimeZone::UTC)
            .unwrap()
            .is_none()
    );
}

#[test]
fn what_it_bears_on_is_what_was_named_then_what_was_mentioned() {
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path();
    std::fs::write(root.join("GOAL.md"), "").unwrap();
    std::fs::write(root.join("named.md"), "").unwrap();
    let text = lines(&[typed("a", T0, "see GOAL.md and named.md and missing.md")]);
    let named = ["muisti".to_string(), "named.md".to_string()];
    let m = make(root, &text, "s", None, &ask(&named), &TimeZone::UTC)
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
fn a_title_with_nothing_to_name_a_file_by_is_refused_before_anything_is_read() {
    let a = Ask {
        title: "!!!",
        ..ask(&[])
    };
    assert!(
        make(
            Path::new("/"),
            "not even json",
            "s",
            None,
            &a,
            &TimeZone::UTC
        )
        .is_err()
    );
}

#[test]
fn a_malformed_transcript_is_refused_rather_than_written_around() {
    let text = format!("{}{{broken\n", lines(&[typed("a", T0, "x")]));
    let err = format!(
        "{:#}",
        make(Path::new("/"), &text, "s", None, &ask(&[]), &TimeZone::UTC).expect_err("refused")
    );
    assert!(err.contains("line 2"), "{err}");
}
