//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! How each registry's answer reads, and the grading of a recorded expiry.

use super::*;

fn grade_of(expires: Option<Day>) -> State {
    grade(
        &Stored {
            secret: "t".into(),
            expires,
        },
        today(),
        "as someone",
    )
    .state
}

#[test]
fn no_expiry_is_ok() {
    assert_eq!(grade_of(None), State::Ok);
}

#[test]
fn far_future_is_ok() {
    assert_eq!(grade_of(Some(today().plus(365))), State::Ok);
}

#[test]
fn inside_two_weeks_is_expiring() {
    assert_eq!(grade_of(Some(today().plus(3))), State::Expiring);
}

#[test]
fn already_past_is_invalid() {
    assert_eq!(grade_of(Some(today().plus(-1))), State::Invalid);
}

#[test]
fn days_left_counts_to_the_expiry() {
    let s = Stored {
        secret:  "hunter2".into(),
        expires: Day::parse("2027-01-01"),
    };
    assert_eq!(s.days_left(Day::parse("2026-12-25").unwrap()), Some(7));
}

fn npm_with(answer: Answer, stored: bool) -> Status {
    let store = if stored { Memory::with("npm", "npm-token", None) } else { Memory::default() };
    check(entry("npm").unwrap(), &store, &Canned(answer), today())
}

#[test]
fn nothing_stored_is_missing() {
    assert_eq!(npm_with((200, None), false).state, State::Missing);
}

#[test]
fn a_401_is_a_rejection_of_the_token_we_hold() {
    assert_eq!(npm_with((401, None), true).state, State::Invalid);
}

#[test]
fn a_200_names_the_holder() {
    let st = npm_with(
        (200, Some(serde_json::json!({"username": "orgrinrt"}))),
        true,
    );
    assert_eq!(st.state, State::Ok);
    assert!(st.detail.contains("orgrinrt"), "{}", st.detail);
}

#[test]
fn no_answer_is_unreachable_rather_than_invalid() {
    assert_eq!(npm_with((0, None), true).state, State::Unreachable);
}

#[test]
fn it_asks_the_registry_rather_than_the_npm_cli() {
    // `Canned::cli` panics, so reaching it fails the test
    assert_eq!(
        npm_with((200, Some(serde_json::json!({"username": "x"}))), true).state,
        State::Ok
    );
}

#[test]
fn each_token_registry_names_its_own_endpoint() {
    for e in ENTRIES {
        let Some(t) = e.token() else { continue };
        assert!(!t.whoami_url.is_empty(), "{}", e.key);
        assert!(
            t.whoami_url.contains(e.key.split('-').next().unwrap()),
            "{}",
            e.key
        );
    }
}

#[test]
fn only_crates_io_sends_the_token_bare() {
    for e in ENTRIES {
        let Some(t) = e.token() else { continue };
        assert_eq!(t.bearer, e.key != "crates-io", "{}", e.key);
    }
}

fn crates_with(answer: Answer) -> Status {
    judge(
        token_of("crates-io"),
        Some(&Stored {
            secret:  "a-token".into(),
            expires: None,
        }),
        &Canned(answer),
        today(),
    )
}

#[test]
fn a_401_from_crates_io_is_a_real_rejection() {
    assert_eq!(crates_with((401, None)).state, State::Invalid);
}

#[test]
fn a_403_is_a_scoped_token_and_must_not_be_rejected() {
    let st = crates_with((403, None));
    assert_ne!(
        st.state,
        State::Invalid,
        "a publish-scoped token was rejected"
    );
    assert!(
        !st.state.needs_work(),
        "a valid scoped token was queued for re-entry"
    );
}

#[test]
fn a_200_names_the_crates_io_user() {
    let st = crates_with((
        200,
        Some(serde_json::json!({"user": {"login": "orgrinrt"}})),
    ));
    assert_eq!(st.state, State::Ok);
    assert!(st.detail.contains("orgrinrt"));
}

#[test]
fn no_answer_from_crates_io_is_unreachable_not_invalid() {
    assert_eq!(crates_with((0, None)).state, State::Unreachable);
}

#[test]
fn a_403_from_jsr_is_no_scoped_token() {
    let st = judge(
        token_of("jsr"),
        Some(&Stored {
            secret:  "t".into(),
            expires: None,
        }),
        &Canned((403, None)),
        today(),
    );
    assert_eq!(st.state, State::Unreachable);
}

#[test]
fn needs_work_covers_the_actionable_states() {
    for s in [State::Missing, State::Invalid, State::Expiring, State::Refused] {
        assert!(s.needs_work(), "{s}");
    }
}

#[test]
fn ok_and_unreachable_do_not_prompt() {
    for s in [State::Ok, State::Unreachable] {
        assert!(!s.needs_work(), "{s}");
    }
}
