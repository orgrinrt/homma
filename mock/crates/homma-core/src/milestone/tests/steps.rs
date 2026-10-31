//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone reports over repositories built here, never the live workspace.
//! The steps that write a row: `freeze`, `pin` and `mark`, and `show` over them.

use super::super::MilestoneError;
use super::super::record::{RECORD_PATH, Row, Source, base, rows};
use super::super::steps::{freeze, mark, pin, show};
use super::Fx;

/// A trunk of four commits, the first of them marked as milestone 1 and
/// committed: `[c1, c2, c3, c4]`.
fn after_one_milestone() -> (Fx, [String; 4]) {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    let c2 = fx.commit("a.txt", "2\n");
    let c3 = fx.commit("a.txt", "3\n");
    let c4 = fx.commit("a.txt", "4\n");
    mark(fx.root(), "dev", &c1, false).unwrap();
    fx.commit_record();
    (fx, [c1, c2, c3, c4])
}

fn date(fx: &Fx, sha: &str) -> String {
    fx.git(&["show", "-s", "--format=%cs", sha])
}

#[test]
fn a_milestone_goes_through_freeze_pin_and_mark_and_the_row_holds_each_revision() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    fx.commit("a.txt", "2\n");
    mark(fx.root(), "dev", &c1, false).unwrap();
    fx.commit_record();
    // the trunk stands at the record commit when the freeze begins, then moves on
    let at_freeze = fx.git(&["rev-parse", "dev"]);
    let opened = freeze(fx.root(), "dev").unwrap();
    assert_eq!(opened.n, Some(2));
    assert_eq!(opened.base.as_deref(), Some(c1.as_str()));
    assert_eq!(opened.freeze.as_deref(), Some(at_freeze.as_str()));
    assert_eq!(
        opened.freeze_at.as_deref(),
        Some(date(&fx, &at_freeze).as_str())
    );
    assert!(opened.head.is_none() && opened.tip().is_none());
    fx.commit_record();
    let c3 = fx.commit("a.txt", "3\n");
    // base is the last tip while the milestone is open
    assert_eq!(
        base(fx.root(), "dev").unwrap(),
        (c1.clone(), Source::Record)
    );
    let pinned = pin(fx.root(), "dev", None).unwrap();
    assert_eq!(pinned.head.as_deref(), Some(c3.as_str()));
    // each step leaves the revisions the earlier ones recorded
    let expected = Row {
        n: Some(2),
        base: Some(c1.clone()),
        freeze: Some(at_freeze.clone()),
        freeze_at: Some(date(&fx, &at_freeze)),
        head: Some(c3.clone()),
        ..Row::default()
    };
    assert_eq!(pinned, expected);
    assert_eq!(rows(fx.root()).unwrap()[1], expected);
    fx.commit_record();
    let c4 = fx.commit("a.txt", "4\n");
    let closed = mark(fx.root(), "dev", &c4, false).unwrap();
    assert_eq!(
        (closed.sha.as_str(), closed.at.as_str()),
        (c4.as_str(), date(&fx, &c4).as_str())
    );
    fx.commit_record();
    let all = rows(fx.root()).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1], Row {
        tip: Some(c4.clone()),
        at: Some(date(&fx, &c4)),
        ..expected
    });
    assert_eq!(base(fx.root(), "dev").unwrap().0, c4);
}

#[test]
fn a_freeze_while_the_last_milestone_is_open_is_refused_and_writes_nothing() {
    let (fx, _) = after_one_milestone();
    freeze(fx.root(), "dev").unwrap();
    let before = std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    assert!(matches!(
        freeze(fx.root(), "dev"),
        Err(MilestoneError::StillOpen(2))
    ));
    assert_eq!(
        std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap(),
        before
    );
}

#[test]
fn the_first_row_has_no_base_where_nothing_is_recorded() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    let row = freeze(fx.root(), "dev").unwrap();
    assert_eq!(row.n, Some(1));
    assert!(row.base.is_none());
    assert_eq!(row.freeze.as_deref(), Some(c1.as_str()));
}

#[test]
fn a_freeze_from_a_trunk_that_does_not_hold_the_last_tip_is_refused() {
    let (fx, [c1, c2, ..]) = after_one_milestone();
    // the second tip is marked, then the trunk is pulled back behind it
    mark(fx.root(), "dev", &c2, false).unwrap();
    fx.commit_record();
    let record = std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    fx.git(&["reset", "-q", "--hard", &c1]);
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(fx.root().join(RECORD_PATH), record).unwrap();
    assert!(matches!(
        freeze(fx.root(), "dev"),
        Err(MilestoneError::NotAncestor { .. })
    ));
}

#[test]
fn a_pin_is_refused_with_no_open_row_twice_off_the_trunk_and_before_the_freeze() {
    let (fx, [c1, c2, c3, _]) = after_one_milestone();
    assert!(matches!(
        pin(fx.root(), "dev", Some(&c3)),
        Err(MilestoneError::NotOpen)
    ));
    // the freeze stands at the record commit after c4: a head before it does not contain it
    freeze(fx.root(), "dev").unwrap();
    fx.commit_record();
    assert!(matches!(
        pin(fx.root(), "dev", Some(&c2)),
        Err(MilestoneError::NotDescendant { .. })
    ));
    assert!(matches!(
        pin(fx.root(), "dev", Some("deadbeef")),
        Err(MilestoneError::NotACommit(_))
    ));
    fx.git(&["switch", "-qc", "side", &c1]);
    let off = fx.commit("b.txt", "side\n");
    fx.git(&["switch", "-q", "dev"]);
    assert!(matches!(
        pin(fx.root(), "dev", Some(&off)),
        Err(MilestoneError::NotOnTrunk { .. })
    ));
    pin(fx.root(), "dev", None).unwrap();
    assert!(matches!(
        pin(fx.root(), "dev", None),
        Err(MilestoneError::AlreadyPinned(2))
    ));
    assert_eq!(
        rows(fx.root()).unwrap()[1].head.as_deref(),
        Some(fx.git(&["rev-parse", "dev"]).as_str())
    );
}

#[test]
fn a_mark_is_refused_before_the_pin_below_the_head_and_on_a_tip_already_taken() {
    let (fx, [c1, ..]) = after_one_milestone();
    let c5 = fx.commit("a.txt", "5\n");
    freeze(fx.root(), "dev").unwrap();
    assert!(matches!(
        mark(fx.root(), "dev", &c5, false),
        Err(MilestoneError::NotPinned(2))
    ));
    let c6 = fx.commit("a.txt", "6\n");
    pin(fx.root(), "dev", Some(&c6)).unwrap();
    // c5 is on the trunk and does not contain the head c6
    assert!(matches!(
        mark(fx.root(), "dev", &c5, false),
        Err(MilestoneError::NotDescendant { .. })
    ));
    // c1 is the first row's tip
    assert!(matches!(
        mark(fx.root(), "dev", &c1, false),
        Err(MilestoneError::AlreadyMarked(_))
    ));
    let row = &rows(fx.root()).unwrap()[1];
    assert!(row.tip().is_none() && row.head.as_deref() == Some(c6.as_str()));
}

#[test]
fn a_row_of_the_old_shape_is_read_as_it_is_and_its_sha_is_its_tip() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    fx.commit("a.txt", "2\n");
    // written by hand: a comment, a key out of order and a key the shape does not name
    let old = format!(
        "# picked by op\n[[milestone]]\n# hand-numbered\nat = \"2026-10-27\"\nsha = \"{c1}\"\nn = \
         1\nnote = \"picked by op\"\n"
    );
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(fx.root().join(RECORD_PATH), &old).unwrap();
    fx.commit_record();
    assert_eq!(
        base(fx.root(), "dev").unwrap(),
        (c1.clone(), Source::Record)
    );
    assert_eq!(rows(fx.root()).unwrap()[0].tip(), Some(c1.as_str()));
    let opened = freeze(fx.root(), "dev").unwrap();
    assert_eq!(
        (opened.n, opened.base.as_deref()),
        (Some(2), Some(c1.as_str()))
    );
    let read = || std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    // the recorded row is left byte for byte as it was written
    assert!(read().starts_with(&old), "{}", read());
    // a hand edit on the open row survives the steps that set its own keys
    let mut edited = read();
    let open_table = edited.rfind("[[milestone]]\n").unwrap() + "[[milestone]]\n".len();
    edited.insert_str(open_table, "# the open one\nextra = 7\n");
    std::fs::write(fx.root().join(RECORD_PATH), edited).unwrap();
    fx.commit_record();
    pin(fx.root(), "dev", None).unwrap();
    let c3 = fx.commit("a.txt", "3\n");
    mark(fx.root(), "dev", &c3, false).unwrap();
    let text = read();
    assert!(text.starts_with(&old), "{text}");
    assert!(text.contains("# the open one\nextra = 7\n"), "{text}");
    let last = &rows(fx.root()).unwrap()[1];
    assert_eq!(last.tip(), Some(c3.as_str()));
    assert!(last.head.is_some() && last.freeze.is_some());
}

#[test]
fn a_milestone_tag_never_gives_the_first_row_a_base() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    fx.commit("a.txt", "2\n");
    fx.git(&["tag", "-a", "milestone/1", &c1, "-m", "m"]);
    let row = freeze(fx.root(), "dev").unwrap();
    assert_eq!(row.n, Some(1));
    assert!(row.base.is_none());
    // the tag still answers the base command
    assert_eq!(base(fx.root(), "dev").unwrap(), (c1, Source::Tag));
}

#[test]
fn a_refused_pin_and_a_refused_mark_write_nothing() {
    let (fx, [c1, c2, ..]) = after_one_milestone();
    freeze(fx.root(), "dev").unwrap();
    let before = std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    assert!(pin(fx.root(), "dev", Some(&c2)).is_err());
    assert!(mark(fx.root(), "dev", &c2, false).is_err());
    assert!(mark(fx.root(), "dev", &c1, false).is_err());
    assert_eq!(
        std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap(),
        before
    );
}

#[test]
fn show_prints_each_row_with_its_epoch_and_review_ranges_and_reads_what_the_trunk_commits() {
    let (fx, [c1, ..]) = after_one_milestone();
    // the first row is the old call: no freeze and no head, so no ranges
    let shown = show(fx.root(), "dev", None).unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].tip.as_deref(), Some(c1.as_str()));
    assert!(shown[0].epoch.is_none() && shown[0].review.is_none());
    let frozen = freeze(fx.root(), "dev").unwrap().freeze.unwrap();
    // the open row is not committed, so the trunk does not show it yet
    assert_eq!(show(fx.root(), "dev", None).unwrap().len(), 1);
    fx.commit_record();
    let pinned = pin(fx.root(), "dev", None).unwrap().head.unwrap();
    fx.commit_record();
    assert_ne!(frozen, pinned);
    let shown = show(fx.root(), "dev", Some(2)).unwrap();
    assert_eq!(shown.len(), 1);
    let s = &shown[0];
    assert_eq!(s.epoch.as_deref(), Some(format!("{c1}..{frozen}").as_str()));
    assert_eq!(
        s.review.as_deref(),
        Some(format!("{c1}..{pinned}").as_str())
    );
    assert!(s.tip.is_none());
    assert!(matches!(
        show(fx.root(), "dev", Some(9)),
        Err(MilestoneError::NoSuchMilestone(9))
    ));
}

#[test]
fn a_row_with_no_base_of_its_own_takes_the_previous_tip_in_show() {
    let (fx, [c1, ..]) = after_one_milestone();
    let c5 = fx.commit("a.txt", "5\n");
    mark(fx.root(), "dev", &c5, false).unwrap();
    fx.commit_record();
    let shown = show(fx.root(), "dev", Some(2)).unwrap();
    assert_eq!(shown[0].base.as_deref(), Some(c1.as_str()));
    assert!(shown[0].epoch.is_none());
}

#[test]
fn a_tagged_mark_of_an_open_row_makes_the_tag_of_its_number() {
    let (fx, _) = after_one_milestone();
    freeze(fx.root(), "dev").unwrap();
    pin(fx.root(), "dev", None).unwrap();
    let c5 = fx.commit("a.txt", "5\n");
    mark(fx.root(), "dev", &c5, true).unwrap();
    assert_eq!(fx.git(&["tag", "--list", "milestone/*"]), "milestone/2");
    assert_eq!(fx.git(&["rev-list", "-n1", "milestone/2"]), c5);
}

/// A trunk of two commits and a record that `make` writes naming the first, put
/// through freeze, pin and mark: the record as written, the text it ends as, and
/// the repository.
fn through_the_steps(make: impl Fn(&str) -> String) -> (String, String, Fx) {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    fx.commit("a.txt", "2\n");
    let old = make(&c1);
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(fx.root().join(RECORD_PATH), &old).unwrap();
    fx.commit_record();
    freeze(fx.root(), "dev").unwrap();
    fx.commit_record();
    pin(fx.root(), "dev", None).unwrap();
    fx.commit_record();
    let c3 = fx.commit("a.txt", "3\n");
    mark(fx.root(), "dev", &c3, false).unwrap();
    let text = std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    (old, text, fx)
}

#[test]
fn the_steps_leave_a_records_line_endings_and_byte_order_mark_as_they_were() {
    for (name, head, eol) in [
        ("crlf", "", "\r\n"),
        ("bom", "\u{feff}", "\n"),
        ("crlf and bom", "\u{feff}", "\r\n"),
    ] {
        let (old, text, fx) = through_the_steps(|c1| {
            format!("{head}[[milestone]]{eol}n = 1{eol}sha = \"{c1}\"{eol}at = \"2026-10-27\"{eol}")
        });
        assert!(text.starts_with(&old), "{name}: {text:?}");
        // every line the steps added ends as the file's own do
        assert_eq!(
            text.matches('\n').count(),
            text.matches(eol).count(),
            "{name}: {text:?}"
        );
        assert_eq!(rows(fx.root()).unwrap().len(), 2, "{name}");
    }
}

#[test]
fn a_comment_after_a_key_a_step_replaces_stays_on_its_line() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(
        fx.root().join(RECORD_PATH),
        format!("[[milestone]]\nn = 1\nfreeze = \"{c1}\"\nat = \"old\" # to be replaced\n"),
    )
    .unwrap();
    fx.commit_record();
    pin(fx.root(), "dev", None).unwrap();
    let c2 = fx.commit("a.txt", "2\n");
    mark(fx.root(), "dev", &c2, false).unwrap();
    let text = std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap();
    assert!(
        text.contains(&format!("at = \"{}\" # to be replaced\n", date(&fx, &c2))),
        "{text}"
    );
}

#[test]
fn a_record_that_is_not_written_as_tables_is_refused_and_left_alone() {
    let fx = Fx::new();
    let c1 = fx.commit("a.txt", "1\n");
    fx.commit("a.txt", "2\n");
    let inline = format!("milestone = [{{ sha = \"{c1}\", at = \"2026-10-27\" }}]\n");
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(fx.root().join(RECORD_PATH), &inline).unwrap();
    fx.commit_record();
    assert!(matches!(
        freeze(fx.root(), "dev"),
        Err(MilestoneError::Record(_))
    ));
    assert_eq!(
        std::fs::read_to_string(fx.root().join(RECORD_PATH)).unwrap(),
        inline
    );
}

#[test]
fn a_freeze_and_a_pin_take_origins_trunk_over_a_stale_local_one() {
    let origin = Fx::new();
    origin.commit("a.txt", "1\n");
    let dir = tempfile::tempdir().unwrap();
    let url = origin.root().to_str().unwrap().to_string();
    let out = std::process::Command::new("git")
        .args(["clone", "-q", "--branch", "dev", &url])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let ahead = origin.commit("a.txt", "2\n");
    let fetched = std::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["fetch", "-q", "origin"])
        .output()
        .unwrap();
    assert!(fetched.status.success(), "{fetched:?}");
    // the local dev is a commit behind origin/dev
    let row = freeze(dir.path(), "dev").unwrap();
    assert_eq!(row.freeze.as_deref(), Some(ahead.as_str()));
    let pinned = pin(dir.path(), "dev", None).unwrap();
    assert_eq!(pinned.head.as_deref(), Some(ahead.as_str()));
}

#[test]
fn an_open_row_with_no_freeze_is_refused_by_a_pin_saying_the_freeze_is_missing() {
    let fx = Fx::new();
    fx.commit("a.txt", "1\n");
    std::fs::create_dir_all(fx.root().join(".homma")).unwrap();
    std::fs::write(fx.root().join(RECORD_PATH), "[[milestone]]\nn = 1\n").unwrap();
    fx.commit_record();
    let err = pin(fx.root(), "dev", None).unwrap_err();
    assert!(err.to_string().contains("no freeze"), "{err}");
}
