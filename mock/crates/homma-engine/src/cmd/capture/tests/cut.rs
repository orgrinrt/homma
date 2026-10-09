//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A transcript line the harness's second writer cut: the start of one object
//! with a complete one after it. Shaped as the four lines of a real long
//! session were, a cut inside a string and then `{"parentUuid"`.

use std::path::Path;

use jiff::tz::TimeZone;
use serde_json::json;

use super::*;
use crate::cmd::capture::cut::Recovered;
use crate::cmd::capture::transcript::{Event, Happened, read};
use crate::cmd::capture::{Ask, make};

const T0: &str = "2026-09-24T14:00:00.000Z";
const T1: &str = "2026-09-24T14:01:00.000Z";
const T2: &str = "2026-09-24T14:02:00.000Z";

/// The start of an assistant line, cut inside the `signature` string where the
/// real one was.
const HEAD: &str = r#"{"parentUuid":"p0","isSidechain":false,"message":{"model":"m","id":"msg_1","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"","signature":"CAQSrggKEAgSGAI4AUIIdGhpbmtpbmcSDBC85ZwFaI/4KTp5Rxo"#;

/// A whole line the person typed, begun as the real ones are.
fn whole(uuid: &str, at: &str, words: &str) -> String {
    format!(
        r#"{{"parentUuid":"p1","isSidechain":false,"promptId":"x","type":"user","uuid":{},"timestamp":{},"origin":{{"kind":"human"}},"message":{{"role":"user","content":{}}}}}"#,
        json!(uuid),
        json!(at),
        json!(words),
    )
}

/// A whole line that is the result of a tool and nobody's words, the kind the
/// first real cut head was followed by.
fn result(uuid: &str, at: &str) -> String {
    format!(
        r#"{{"parentUuid":"p1","isSidechain":false,"promptId":"x","type":"user","uuid":{},"timestamp":{},"message":{{"role":"user","content":[{{"tool_use_id":"toolu_1","type":"tool_result","content":"252G"}}]}}}}"#,
        json!(uuid),
        json!(at),
    )
}

fn events(text: &str) -> anyhow::Result<Vec<Event>> {
    read(text).map(|t| t.events)
}

fn said(e: &Event) -> &str {
    match &e.what {
        Happened::Said(w) => w,
        other => panic!("not said: {other:?}"),
    }
}

/// Line 2 is the case, between two lines of the person's.
fn around(line: &str) -> String {
    format!(
        "{}\n{line}\n{}\n",
        typed("a", T0, "first"),
        typed("c", T2, "last")
    )
}

fn refused(line: &str) -> String {
    format!(
        "{:#}",
        events(&around(line)).expect_err("a line with nothing whole to read is refused")
    )
}

#[test]
fn a_line_cut_short_then_whole_is_read_as_the_whole_line() {
    let text = around(&format!(
        "{HEAD}{}",
        whole("b", T1, "the one after the cut")
    ));
    let t = read(&text).expect("a cut write before a whole line is read");
    let got: Vec<&str> = t.events.iter().map(said).collect();
    assert_eq!(got, ["first", "the one after the cut", "last"]);
    // It stands where the line stood in the file, and the watermark finds it.
    assert_eq!(t.events[1].line, 1);
    assert_eq!(t.line_of("b"), Some(1));
    assert_eq!(t.last.as_deref(), Some("c"));
}

#[test]
fn a_line_with_no_whole_object_ending_it_is_still_refused_by_number() {
    let tail = whole("b", T1, "x");
    let cases = [
        // Only the cut write, no line after it.
        HEAD.to_string(),
        // A whole object with more text after it, so no object ends the line.
        format!("{tail}and then the rest of something"),
        // Two cut writes, and nothing whole.
        format!("{HEAD}{HEAD}"),
        // A cut write, then a whole line cut at its own end.
        format!("{HEAD}{}", &tail[.. tail.len() - 6]),
        // Text with no brace on it.
        "not json".to_string(),
    ];
    for line in cases {
        let err = refused(&line);
        assert!(
            err.contains("transcript line 2 is not JSON"),
            "{line}: {err}"
        );
    }
}

#[test]
fn two_whole_lines_run_together_are_refused_by_number() {
    // Not a write that was cut: both are whole, and reading the second alone
    // would drop a complete record.
    let line = format!("{}{}", whole("b", T1, "one"), whole("d", T1, "two"));
    let err = refused(&line);
    assert!(err.contains("transcript line 2 is not JSON"), "{err}");
    // The control: the same two lines, each on its own, are both read.
    let apart = format!(
        "{}\n{}\n{}\n{}\n",
        typed("a", T0, "first"),
        whole("b", T1, "one"),
        whole("d", T1, "two"),
        typed("c", T2, "last"),
    );
    assert_eq!(events(&apart).expect("reads").len(), 4);
}

#[test]
fn the_recovered_object_is_read_as_an_ordinary_line() {
    // The person's words in a recovered line are captured, through the same
    // path as any other: stamped, quoted, and the capture ending on its uuid.
    let text = format!(
        "{}\n{HEAD}{}\n",
        typed("a", T0, "first"),
        whole("b", T1, "said as a cut write landed before it"),
    );
    let t = read(&text).expect("reads");
    let ask = Ask {
        title:    "Cut",
        session:  None,
        since:    None,
        bears_on: &[],
        into:     None,
        store:    Path::new(".data/op-responses"),
        running:  None,
    };
    let made = make(Path::new("/"), &t, "s", None, &ask, &TimeZone::UTC)
        .expect("made")
        .expect("something said");
    assert_eq!((made.said, made.asked), (2, 0));
    assert!(
        made.body
            .contains("> said as a cut write landed before it\n"),
        "{}",
        made.body
    );
    assert!(made.body.contains("\nthrough: b\n"), "{}", made.body);

    // And an ordinary line it stays: the result of a tool that follows a cut
    // write is not the person's, recovered or not.
    let other = format!("{HEAD}{}\n{}\n", result("r", T1), typed("c", T2, "last"));
    let t = read(&other).expect("reads");
    let got: Vec<&str> = t.events.iter().map(said).collect();
    assert_eq!(got, ["last"]);
    assert_eq!(t.line_of("r"), Some(0));
}

#[test]
fn every_recovered_line_is_reported_by_number_and_bytes() {
    let second = r#"{"parentUuid":"p9","isSidechain":false,"message":{"role":"assistant","content":[{"type":"text","text":"half a reply"#;
    let text = format!(
        "{}\n{HEAD}{}\n{}\n{second}{}\n",
        typed("a", T0, "first"),
        whole("b", T1, "one"),
        typed("c", T1, "between"),
        whole("d", T2, "two"),
    );
    let t = read(&text).expect("reads");
    assert_eq!(t.recovered, [
        Recovered {
            line:    2,
            dropped: HEAD.len(),
        },
        Recovered {
            line:    4,
            dropped: second.len(),
        },
    ]);
    assert_eq!(
        t.recovered[0].to_string(),
        format!(
            "transcript line 2 does not parse whole: read the object that ends it and dropped {} \
             bytes before it",
            HEAD.len()
        )
    );
    // One byte is a byte.
    let one = format!("x{}", whole("e", T1, "y"));
    let t = read(&around(&one)).expect("reads");
    assert_eq!(
        t.recovered[0].to_string(),
        "transcript line 2 does not parse whole: read the object that ends it and dropped 1 byte before it"
    );
    // The control: a transcript with nothing cut has nothing to report.
    let clean = read(&around(&whole("b", T1, "one"))).expect("reads");
    assert!(clean.recovered.is_empty());
}
