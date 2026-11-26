//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

fn at(s: &str) -> Timestamp {
    s.parse().unwrap()
}

const NOON: &str = "2026-11-23T12:00:00Z";

fn held_by(session: &str, expires: &str) -> Lease {
    Lease {
        handle:   "inge".into(),
        session:  session.into(),
        job:      "identity-lane".into(),
        taken:    at("2026-11-23T11:00:00Z"),
        expires:  at(expires),
        replaces: None,
    }
}

fn take<'a>(session: &'a str, replaces: Option<&'a str>) -> Ask<'a> {
    Ask::Take {
        session,
        job: "next-job",
        replaces,
    }
}

#[test]
fn a_free_identity_is_taken_for_the_asking_session() {
    let out = decide(None, "inge", take("s1", None), at(NOON), 120).unwrap();
    assert_eq!(
        out,
        Outcome::Write(Lease {
            handle:   "inge".into(),
            session:  "s1".into(),
            job:      "next-job".into(),
            taken:    at(NOON),
            expires:  at("2026-11-23T14:00:00Z"),
            replaces: None,
        })
    );
}

#[test]
fn a_live_lease_of_another_session_refuses_a_take_and_names_the_holder() {
    let held = held_by("s1", "2026-11-23T13:00:00Z");
    let err = decide(Some(&held), "inge", take("s2", None), at(NOON), 120).unwrap_err();
    assert_eq!(err, Refusal::Held {
        holder:  "s1".into(),
        job:     "identity-lane".into(),
        expires: at("2026-11-23T13:00:00Z"),
    });
    assert!(err.to_string().contains("--replaces s1"));
}

#[test]
fn a_takeover_of_a_live_lease_names_the_holder_it_replaces() {
    let held = held_by("s1", "2026-11-23T13:00:00Z");
    let Outcome::Write(l) =
        decide(Some(&held), "inge", take("s2", Some("s1")), at(NOON), 60).unwrap()
    else {
        panic!("a named takeover writes a record");
    };
    assert_eq!(
        (l.session.as_str(), l.replaces.as_deref()),
        ("s2", Some("s1"))
    );
    assert_eq!(l.taken, at(NOON));
}

#[test]
fn a_takeover_naming_somebody_other_than_the_holder_is_refused() {
    // Expired or not: a take that names a session means that session, and a
    // mismatch says the asker's picture is out of date.
    for expires in ["2026-11-23T13:00:00Z", "2026-11-23T11:30:00Z"] {
        let held = held_by("s1", expires);
        let err = decide(Some(&held), "inge", take("s2", Some("s9")), at(NOON), 60).unwrap_err();
        assert_eq!(err, Refusal::ReplacesAnother {
            named:  "s9".into(),
            holder: "s1".into(),
        });
    }
}

#[test]
fn a_lapsed_lease_is_taken_over_and_the_record_says_whose_it_was() {
    let held = held_by("s1", "2026-11-23T11:59:59Z");
    let Outcome::Write(l) = decide(Some(&held), "inge", take("s2", None), at(NOON), 60).unwrap()
    else {
        panic!("a lapsed lease is free to take");
    };
    assert_eq!(l.replaces.as_deref(), Some("s1"));
    assert_eq!(l.session, "s2");
}

#[test]
fn a_lease_lapses_at_its_expiry_exactly() {
    let held = held_by("s1", NOON);
    assert!(!held.is_live(at(NOON)));
    assert!(held.is_live(at("2026-11-23T11:59:59Z")));
}

#[test]
fn the_holder_taking_again_renews_and_keeps_when_it_took_it() {
    let held = held_by("s1", "2026-11-23T12:30:00Z");
    let Outcome::Write(l) = decide(Some(&held), "inge", take("s1", None), at(NOON), 120).unwrap()
    else {
        panic!("the holder's take is a renewal");
    };
    assert_eq!(l.taken, held.taken);
    assert_eq!(l.expires, at("2026-11-23T14:00:00Z"));
    assert_eq!(l.job, "next-job");
}

#[test]
fn a_renewal_is_the_holders_alone() {
    let held = held_by("s1", "2026-11-23T12:30:00Z");
    let Outcome::Write(l) = decide(
        Some(&held),
        "inge",
        Ask::Renew {
            session: "s1",
        },
        at(NOON),
        30,
    )
    .unwrap() else {
        panic!("the holder renews");
    };
    assert_eq!(l.expires, at("2026-11-23T12:30:00Z"));
    assert_eq!(l.job, held.job);
    assert_eq!(
        decide(
            Some(&held),
            "inge",
            Ask::Renew {
                session: "s2",
            },
            at(NOON),
            30
        )
        .unwrap_err(),
        Refusal::NotHolder {
            holder: Some("s1".into()),
        }
    );
    assert_eq!(
        decide(
            None,
            "inge",
            Ask::Renew {
                session: "s1",
            },
            at(NOON),
            30
        )
        .unwrap_err(),
        Refusal::NotHolder {
            holder: None,
        }
    );
}

#[test]
fn a_release_removes_the_holders_record_and_refuses_anybody_elses() {
    let held = held_by("s1", "2026-11-23T12:30:00Z");
    assert_eq!(
        decide(
            Some(&held),
            "inge",
            Ask::Release {
                session: "s1",
            },
            at(NOON),
            30
        ),
        Ok(Outcome::Remove)
    );
    assert_eq!(
        decide(
            Some(&held),
            "inge",
            Ask::Release {
                session: "s2",
            },
            at(NOON),
            30
        ),
        Err(Refusal::NotHolder {
            holder: Some("s1".into()),
        })
    );
    assert_eq!(
        decide(
            None,
            "inge",
            Ask::Release {
                session: "s1",
            },
            at(NOON),
            30
        ),
        Ok(Outcome::Nothing)
    );
}

#[test]
fn a_record_reads_back_as_it_was_written() {
    let mut l = held_by("s1", "2026-11-23T13:00:00Z");
    l.replaces = Some("s0".into());
    let text = l.render();
    assert!(
        text.contains("expires = \"2026-11-23T13:00:00Z\""),
        "{text}"
    );
    assert_eq!(Lease::parse(&text).unwrap(), l);
}
