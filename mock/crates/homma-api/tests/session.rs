//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a session is, and which rules bind it.
//!
//! The refusals matter more than the reads: a role file that is merely
//! misspelt must not be read as "no role", which would load every card.

use homma_api::rule;
use homma_api::session::{Place, Session, SessionRole};

/// A rule with the given meta lines, parsed.
fn meta(extra: &str) -> homma_api::RuleMeta {
    let src = format!("---\ntopics: [a]\nfires: \"x\"\nkind: reflex\n{extra}\n---\n\nBody.");
    rule::parse(&src).unwrap().meta
}

#[test]
fn the_role_file_is_read_for_each_role() {
    for (text, role) in [
        ("coordinator", SessionRole::Coordinator),
        ("helper", SessionRole::Helper),
        ("helper\n", SessionRole::Helper),
        ("  coordinator  \n", SessionRole::Coordinator),
    ] {
        let s = Session::from_parts(Some(text), None).unwrap();
        assert_eq!(s.role, Some(role), "{text:?}");
    }
}

#[test]
fn no_role_file_is_no_role() {
    assert_eq!(Session::from_parts(None, None).unwrap().role, None);
}

#[test]
fn a_role_file_that_holds_anything_else_is_refused_by_name() {
    for text in ["", "\n", "helpr", "Coordinator", "coordinator helper", "kaski"] {
        let err = Session::from_parts(Some(text), None).unwrap_err();
        assert_eq!(err.got(), text.trim(), "{text:?}");
        assert!(
            err.to_string().contains("coordinator"),
            "lists what is expected: {err}"
        );
    }
}

#[test]
fn the_cloud_variable_is_cloud_only_when_exactly_one() {
    let place = |v| Session::from_parts(None, v).unwrap().place;
    assert_eq!(place(Some("1")), Place::Cloud);
    // The control: `0` is written on purpose to say it is not a cloud container.
    assert_eq!(place(Some("0")), Place::Local);
    assert_eq!(place(Some("")), Place::Local);
    assert_eq!(place(Some("true")), Place::Local);
    assert_eq!(place(None), Place::Local);
}

#[test]
fn words_round_trip() {
    for r in SessionRole::ALL {
        assert_eq!(r.word().parse::<SessionRole>(), Ok(r));
    }
    for p in Place::ALL {
        assert_eq!(p.word().parse::<Place>(), Ok(p));
    }
}

#[test]
fn a_rule_that_says_nothing_is_admitted_by_every_session() {
    let rule = meta("");
    for place in Place::ALL {
        for role in [None, Some(SessionRole::Coordinator), Some(SessionRole::Helper)] {
            assert!(
                Session {
                    role,
                    place
                }
                .admits(&rule),
                "{role:?} {place:?}"
            );
        }
    }
}

#[test]
fn a_rule_naming_a_role_is_admitted_by_that_role_and_by_no_role_only() {
    let coordinator_only = meta("roles: [coordinator]");
    let in_cloud = |role| {
        Session {
            role,
            place: Place::Cloud,
        }
    };
    assert!(in_cloud(Some(SessionRole::Coordinator)).admits(&coordinator_only));
    assert!(!in_cloud(Some(SessionRole::Helper)).admits(&coordinator_only));
    // The control: a session with no role file takes every role.
    assert!(in_cloud(None).admits(&coordinator_only));

    let helper_only = meta("roles: [helper]");
    assert!(in_cloud(Some(SessionRole::Helper)).admits(&helper_only));
    assert!(!in_cloud(Some(SessionRole::Coordinator)).admits(&helper_only));

    let both = meta("roles: [coordinator, helper]");
    assert!(in_cloud(Some(SessionRole::Coordinator)).admits(&both));
    assert!(in_cloud(Some(SessionRole::Helper)).admits(&both));
}

#[test]
fn a_rule_naming_a_place_is_admitted_there_only() {
    let local = meta("where: local");
    let cloud = meta("where: cloud");
    for role in [None, Some(SessionRole::Coordinator), Some(SessionRole::Helper)] {
        let at = |place| {
            Session {
                role,
                place,
            }
        };
        assert!(at(Place::Local).admits(&local));
        assert!(!at(Place::Cloud).admits(&local));
        assert!(at(Place::Cloud).admits(&cloud));
        assert!(!at(Place::Local).admits(&cloud));
    }
}

#[test]
fn role_and_place_both_have_to_fit() {
    let rule = meta("roles: [coordinator]\nwhere: local");
    let s = |role, place| {
        Session {
            role: Some(role),
            place,
        }
    };
    assert!(s(SessionRole::Coordinator, Place::Local).admits(&rule));
    assert!(!s(SessionRole::Coordinator, Place::Cloud).admits(&rule));
    assert!(!s(SessionRole::Helper, Place::Local).admits(&rule));
}

/// The whole space, written out by hand: four role tags by three place tags,
/// each with the answer for a session of no role, a coordinator and a helper,
/// in the cloud and then locally.
///
/// A literal table rather than a loop computing the answer a second way, since
/// a second `admits` would agree with the first one's mistakes.
#[test]
fn every_cell_of_roles_by_place_is_what_the_table_says() {
    const T: bool = true;
    const F: bool = false;
    // Rows: no role, coordinator, helper. Columns: cloud, local.
    let table: [(&str, [[bool; 2]; 3]); 12] = [
        ("", [[T, T], [T, T], [T, T]]),
        ("where: cloud", [[T, F], [T, F], [T, F]]),
        ("where: local", [[F, T], [F, T], [F, T]]),
        ("roles: [coordinator]", [[T, T], [T, T], [F, F]]),
        ("roles: [coordinator]\nwhere: cloud", [[T, F], [T, F], [
            F, F,
        ]]),
        ("roles: [coordinator]\nwhere: local", [[F, T], [F, T], [
            F, F,
        ]]),
        ("roles: [helper]", [[T, T], [F, F], [T, T]]),
        ("roles: [helper]\nwhere: cloud", [[T, F], [F, F], [T, F]]),
        ("roles: [helper]\nwhere: local", [[F, T], [F, F], [F, T]]),
        ("roles: [coordinator, helper]", [[T, T], [T, T], [T, T]]),
        ("roles: [coordinator, helper]\nwhere: cloud", [
            [T, F],
            [T, F],
            [T, F],
        ]),
        ("roles: [coordinator, helper]\nwhere: local", [
            [F, T],
            [F, T],
            [F, T],
        ]),
    ];
    let roles = [None, Some(SessionRole::Coordinator), Some(SessionRole::Helper)];
    for (tags, expected) in table {
        let rule = meta(tags);
        for (r, role) in roles.into_iter().enumerate() {
            for (p, place) in Place::ALL.into_iter().enumerate() {
                assert_eq!(
                    Session {
                        role,
                        place
                    }
                    .admits(&rule),
                    expected[r][p],
                    "{tags:?} for {role:?} in {place:?}"
                );
            }
        }
    }
}
