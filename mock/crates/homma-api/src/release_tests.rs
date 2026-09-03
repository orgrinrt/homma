//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The release types away from their declarations: the version arithmetic,
//! the level steps, the finding severities and the gate outcome.

use super::*;

#[test]
fn a_major_before_one_point_zero_is_a_minor() {
    assert_eq!(
        Version::new(0, 4, 1).bumped(Level::Major),
        Version::new(0, 5, 0)
    );
    assert_eq!(
        Version::new(1, 4, 1).bumped(Level::Major),
        Version::new(2, 0, 0)
    );
}

#[test]
fn a_patch_and_a_minor_move_the_part_they_name_and_reset_below() {
    assert_eq!(
        Version::new(0, 4, 1).bumped(Level::Patch),
        Version::new(0, 4, 2)
    );
    assert_eq!(
        Version::new(0, 4, 1).bumped(Level::Minor),
        Version::new(0, 5, 0)
    );
    assert_eq!(
        Version::new(2, 4, 1).bumped(Level::Minor),
        Version::new(2, 5, 0)
    );
}

#[test]
fn a_prerelease_is_dropped_by_any_bump_and_sorts_before_its_release() {
    let pre: Version = "1.2.0-alpha.1".parse().unwrap();
    assert_eq!(pre.bumped(Level::Patch), Version::new(1, 2, 1));
    assert_eq!(pre.bumped(Level::Minor), Version::new(1, 3, 0));
    assert_eq!(pre.bumped(Level::Major), Version::new(2, 0, 0));
    assert!(pre < Version::new(1, 2, 0));
    assert!(Version::new(1, 1, 9) < pre);
}

#[test]
fn a_version_parses_with_or_without_the_v_and_refuses_the_rest() {
    assert_eq!("v0.2.2".parse::<Version>().unwrap(), Version::new(0, 2, 2));
    assert_eq!("0.2.2".parse::<Version>().unwrap(), Version::new(0, 2, 2));
    assert!("0.2".parse::<Version>().is_err());
    assert!("0.2.2.1".parse::<Version>().is_err());
    assert!("0.2.2-".parse::<Version>().is_err());
    assert!("archive/x".parse::<Version>().is_err());
    assert_eq!(Version::new(3, 0, 0).to_string(), "3.0.0");
}

#[test]
fn the_smallest_successor_is_exactly_one_step() {
    let v = Version::new(0, 2, 2);
    assert!(v.is_smallest_successor(&Version::new(0, 2, 3), Level::Patch));
    assert!(!v.is_smallest_successor(&Version::new(0, 2, 4), Level::Patch));
    assert!(!v.is_smallest_successor(&Version::new(0, 3, 0), Level::Patch));
}

#[test]
fn a_level_parses_its_three_words_and_nothing_else() {
    assert_eq!("patch".parse::<Level>().unwrap(), Level::Patch);
    assert_eq!("major".parse::<Level>().unwrap(), Level::Major);
    assert!("Patch".parse::<Level>().is_err());
    assert!("release".parse::<Level>().is_err());
}

fn run() -> GateRun {
    let mut tests = StepOutcome {
        step:    Step::Tests,
        passed:  true,
        skipped: false,
        numbers: BTreeMap::new(),
        log:     "ok".into(),
    };
    tests.numbers.insert("summary".into(), "41/41".into());
    let docs = StepOutcome {
        step:    Step::Docs,
        passed:  false,
        skipped: false,
        numbers: BTreeMap::from([("summary".to_string(), "97%".to_string())]),
        log:     String::new(),
    };
    let steps = vec![
        StepOutcome::skipped(Step::Format),
        tests,
        StepOutcome::skipped(Step::Deny),
        docs,
    ];
    GateRun {
        repo: "notko".into(),
        sha: "abc123".into(),
        ran_at: "2026-09-02T21:00:00Z".into(),
        verdict: GateRun::verdict_of(&steps),
        steps,
    }
}

#[test]
fn docs_failing_does_not_redden_but_a_blocking_step_does() {
    assert_eq!(run().verdict, Verdict::Green);
    let mut red = run();
    red.steps[1].passed = false;
    assert_eq!(GateRun::verdict_of(&red.steps), Verdict::Red);
}

#[test]
fn a_run_round_trips_through_its_record_and_the_kind_checks_it() {
    let r = run();
    let record = r.to_record();
    record.check(&GateRun::kind()).unwrap();
    assert_eq!(GateRun::from_record(&record).unwrap(), r);
}

#[test]
fn a_record_of_another_kind_is_refused() {
    let mut record = run().to_record();
    record.kind = "message".into();
    assert!(GateRun::from_record(&record).is_err());
    let mut bad = run().to_record();
    bad.attrs
        .insert("verdict".into(), Attr::Text("amber".into()));
    assert!(GateRun::from_record(&bad).is_err());
}

#[test]
fn the_summary_skips_skipped_steps_and_names_a_failure_without_a_number() {
    assert_eq!(run().summary(), "tests 41/41, docs 97%");
    let mut r = run();
    r.steps[1].numbers.clear();
    r.steps[1].passed = false;
    assert_eq!(r.summary(), "tests failed, docs 97%");
}

#[test]
fn a_badge_serialises_in_the_endpoint_shape() {
    let json = serde_json::to_string(&Badge::new("tests", "41/41", "green")).unwrap();
    assert_eq!(
        json,
        r#"{"schemaVersion":1,"label":"tests","message":"41/41","color":"green"}"#
    );
}

#[test]
fn only_error_and_fatal_block() {
    assert!(!CheckSeverity::Warn.blocks());
    assert!(CheckSeverity::Error.blocks());
    assert!(CheckSeverity::Fatal.blocks());
}
