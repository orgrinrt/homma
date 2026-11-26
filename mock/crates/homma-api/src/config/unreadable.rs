//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The check that refuses a registry string which would inject structure into
//! anything generated from it.

/// The reported unsafe fields of a set of entries, keyed by handle.
///
/// **Derived from the serialised form rather than enumerated.** The list used to
/// be written out field by field, and two consecutive rounds added a field that
/// did not reach it. The test guarding against the first omission was itself a
/// hand-written list and could not see the second. Serialising sees every field
/// including the ones nobody has added yet, which is the only version of this
/// that does not need somebody to remember, and it costs one serialisation per
/// entry on a file read.
///
/// **An entry that will not serialise is reported, not skipped.** A check that
/// cannot read a thing has not cleared it, and this one exists to refuse
/// hostile strings, so clearing what it could not read would disable it for
/// that whole entry. No field of an [`Identity`] can fail today, every optional
/// one being skipped when absent, which is why the branch is cheap to get right
/// now rather than under whichever later field makes it reachable.
///
/// Generic over the entry for that same reason: the unreadable arm needs a
/// caller a test can reach, and a test that reimplemented this match to get
/// around `Identity` would be checking its own copy rather than this one.
///
/// [`Identity`]: super::identity::Identity
pub(super) fn unsafe_strings_of<'a, T: serde::Serialize + 'a>(
    entries: impl IntoIterator<Item = (&'a String, &'a T)>,
) -> Vec<(String, String)> {
    let mut bad = Vec::new();
    for (handle, entry) in entries {
        match control_character_fields(entry) {
            Ok(fields) => bad.extend(fields.into_iter().map(|f| (handle.clone(), f))),
            Err(Unreadable) => bad.push((handle.clone(), UNREADABLE.to_string())),
        }
    }
    bad
}

/// What [`Workspace::unsafe_strings`] reports for an entry it could not read.
///
/// A field path like any other, so both callers render and refuse it without
/// knowing this case exists, and an operator sees the entry named.
///
/// [`Workspace::unsafe_strings`]: super::Workspace::unsafe_strings
pub const UNREADABLE: &str = "<unserialisable>";

/// An entry could not be serialised, so nothing about it can be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the entry could not be serialised, so its strings cannot be checked")
    }
}

impl std::error::Error for Unreadable {}

/// The dotted paths of every string in one entry that carries a control
/// character.
///
/// Generic over `Serialize` rather than taking [`Identity`], because the
/// unreadable branch is not reachable through `Identity` and a branch no test
/// can reach is a branch no test covers.
///
/// [`Identity`]: super::identity::Identity
fn control_character_fields<T: serde::Serialize>(entry: &T) -> Result<Vec<String>, Unreadable> {
    let value = toml::Value::try_from(entry).map_err(|_| Unreadable)?;
    let mut found = Vec::new();
    walk_for_control_characters(&value, "", &mut |field| found.push(field.to_string()));
    Ok(found)
}

/// Every string anywhere in a serialised value, with its dotted path, reported
/// when it carries a control character.
///
/// Recursive because a field may be a table or an array, and a check that
/// handled only the flat case would be the same omission one level down.
fn walk_for_control_characters(value: &toml::Value, path: &str, found: &mut impl FnMut(&str)) {
    match value {
        toml::Value::String(s) => {
            if s.chars().any(|c| c.is_control()) {
                found(path);
            }
        },
        toml::Value::Table(t) => {
            for (k, v) in t {
                let next = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                walk_for_control_characters(v, &next, found);
            }
        },
        toml::Value::Array(a) => {
            for v in a {
                walk_for_control_characters(v, path, found);
            }
        },
        _ => {},
    }
}

#[cfg(test)]
mod an_entry_that_cannot_be_read_is_not_cleared {
    use super::*;

    /// Serialises to nothing TOML can hold. A TOML table's keys are strings,
    /// so a map keyed on anything else has no representation and `try_from`
    /// refuses it. Nothing about the entry can then be walked.
    ///
    /// Declared here rather than reached through [`Identity`] because no field
    /// of `Identity` can fail: every optional one is skipped when absent, and
    /// an absent option is not a serialisation failure. That is the whole
    /// reason [`control_character_fields`] is generic.
    ///
    /// [`Identity`]: super::super::identity::Identity
    #[derive(serde::Serialize)]
    struct WillNotSerialise {
        keyed_on_a_number: std::collections::BTreeMap<u8, String>,
    }

    impl WillNotSerialise {
        fn new() -> Self {
            Self {
                keyed_on_a_number: std::collections::BTreeMap::from([(1u8, "x".to_string())]),
            }
        }
    }

    #[derive(serde::Serialize)]
    struct Readable {
        nickname: String,
        nested:   Nested,
    }

    #[derive(serde::Serialize)]
    struct Nested {
        note: String,
    }

    #[test]
    fn an_unserialisable_entry_is_refused_rather_than_passed() {
        assert_eq!(
            control_character_fields(&WillNotSerialise::new()),
            Err(Unreadable)
        );
    }

    #[test]
    fn and_it_reaches_the_caller_as_a_reported_field_rather_than_as_silence() {
        // The property that matters to the two callers, both of which refuse on
        // a non-empty list and render `handle`.field. Asserted through the
        // reported path rather than through the error type, because it is the
        // reporting that the fail-open version got wrong.
        let handle = "vesta".to_string();
        let entries = std::collections::BTreeMap::from([(handle.clone(), WillNotSerialise::new())]);
        assert_eq!(unsafe_strings_of(&entries), vec![(
            handle,
            UNREADABLE.to_string()
        )]);
    }

    #[test]
    fn a_readable_entry_still_reports_its_control_characters_by_field() {
        // The first control. Failing closed must not be a way of failing on
        // everything: a readable entry is still walked, and the path it reports
        // is still the dotted one an operator can go and find.
        let got = control_character_fields(&Readable {
            nickname: "vou\nti".to_string(),
            nested:   Nested {
                note: "fine".to_string(),
            },
        })
        .expect("this one serialises");
        assert_eq!(got, vec!["nickname".to_string()]);

        let got = control_character_fields(&Readable {
            nickname: "fine".to_string(),
            nested:   Nested {
                note: "one\ttwo".to_string(),
            },
        })
        .expect("this one serialises");
        assert_eq!(got, vec!["nested.note".to_string()]);
    }

    #[test]
    fn a_readable_clean_entry_reports_nothing() {
        // The second control, and the one that would catch a fail-closed that
        // had become fail-always.
        assert_eq!(
            control_character_fields(&Readable {
                nickname: "vesta".to_string(),
                nested:   Nested {
                    note: "fine".to_string(),
                },
            }),
            Ok(Vec::new())
        );
    }

    #[test]
    fn the_reported_path_is_not_a_word_a_real_field_could_take() {
        // `UNREADABLE` shares the field-path channel, so an entry with a field
        // genuinely named that would be indistinguishable. Angle brackets are
        // not legal in a TOML bare key, and a quoted key carrying them is not
        // something serde derives.
        assert!(UNREADABLE.starts_with('<') && UNREADABLE.ends_with('>'));
    }
}

#[cfg(test)]
mod tests {
    use super::super::identity::{Identity, Role};
    use super::super::{MINIMAL, WITH_ORG, Workspace};

    #[test]
    fn a_control_character_in_any_registry_string_is_reported() {
        // A newline here becomes an arbitrary key in generated frontmatter.
        let mut w = Workspace::parse(MINIMAL).unwrap();
        let mut id = Identity::new(Role::Hand, "pike");
        id.nickname = Some("Pike\nmemory: project".into());
        w.org.insert("pike".into(), id);
        let bad = w.unsafe_strings();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0], ("pike".to_string(), "nickname".to_string()));
    }

    /// Every free-form string an entry carries, by dotted path.
    ///
    /// **A literal list, on purpose.** The previous version of this test derived
    /// the expected set with the same walker the production code uses, so both
    /// sides of its equality were one computation on one value and it could not
    /// fail: deleting the array arm, so the one array field went unscanned,
    /// left it green. A list is worse to maintain and is the only thing here
    /// that a wrong walker cannot satisfy.
    ///
    /// `role` is absent because it is a closed vocabulary that serialises as a
    /// string and cannot carry a control character.
    const CORRUPTIBLE: &[&str] = &[
        "handle",
        "aliases",
        "nickname",
        "full_name",
        "domain",
        "git_name",
        "git_email",
        "committer_email",
        "committer_name",
        "workspace",
        "session",
        "repos",
    ];

    // **This does not close at compile time, and four rounds spent trying to
    // make it are why that is stated first.** A field added to `Identity`, bound
    // here as `field: _`, and given `None` in the literal below leaves the suite
    // green with no warnings. So does `..` in place of the binding. No attribute
    // reaches either, because neither produces an unused variable, so `deny`
    // below is silent for both.
    //
    // The guard's own error is what routes a reader there. `E0027` fires when a
    // field is unnamed, and rustc's three suggested remedies are, verbatim,
    // `, forge_account }`, `, forge_account: _ }` and `, .. }`. Two of the three
    // disarm this.
    //
    // **The guarantee that covers an unknown field is `unsafe_strings` deriving
    // its list from the serialised form**, which sees every field including the
    // ones nobody has added yet and needs no author to remember anything. That
    // is the mechanism to keep sound. This test is an aid to it.
    //
    // What it does buy, which is real and is the reason it stays:
    //
    // - The destructure makes a field added the obvious way a compile error
    //   rather than a silent omission, so the escape has to be typed on purpose.
    // - `deny(unused_variables)` closes the case where a field is bound and then
    //   classified in neither list, which produced a green suite and one warning
    //   in an earlier round, because the workspace carries no `[lints]` table.
    // - The literal is kept **beside** the destructure rather than instead of
    //   it, so `E0063` demands a value and the assertion below demands that the
    //   value be one the check should catch.
    //
    // A field can still be classified wrongly on purpose, and it can still be
    // omitted by anybody who writes `_` or `..`.
    #[deny(unused_variables)]
    #[test]
    fn every_field_is_classified_as_free_form_or_not() {
        let Identity {
            // Not free-form. A closed vocabulary and a flag; neither can carry a
            // control character.
            role,
            staffed,
            // Free-form. Each must appear in `CORRUPTIBLE`, and each must be
            // populated with a hostile value in the test below.
            handle,
            aliases,
            nickname,
            full_name,
            domain,
            git_name,
            git_email,
            committer_email,
            committer_name,
            workspace,
            session,
            repos,
        } = Identity::new(Role::Hand, "pike");

        // Bound, so deleting a line above without deleting its name here is also
        // a compile error rather than a silent narrowing.
        let not_free_form = [format!("{role:?}"), format!("{staffed:?}")];
        let free_form = [
            ("handle", format!("{handle:?}")),
            ("aliases", format!("{aliases:?}")),
            ("nickname", format!("{nickname:?}")),
            ("full_name", format!("{full_name:?}")),
            ("domain", format!("{domain:?}")),
            ("git_name", format!("{git_name:?}")),
            ("git_email", format!("{git_email:?}")),
            ("committer_email", format!("{committer_email:?}")),
            ("committer_name", format!("{committer_name:?}")),
            ("workspace", format!("{workspace:?}")),
            ("session", format!("{session:?}")),
            ("repos", format!("{repos:?}")),
        ];
        assert_eq!(not_free_form.len(), 2);

        let named: Vec<&str> = free_form.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            named, CORRUPTIBLE,
            "a field classified free-form here must appear in CORRUPTIBLE, which is \
             what the hostile-entry test asserts production reports"
        );
    }

    #[test]
    fn every_free_form_string_is_reported_when_it_carries_a_control_character() {
        // **A struct literal, and that is the gate.** Written as
        // `Identity::new` plus assignments, a field added later is simply not
        // populated, `skip_serializing_if` keeps it out of the serialised value,
        // and nothing here or in production ever sees it. A literal is
        // exhaustive: `E0063` refuses to compile until the new field is given a
        // value, that value carries a control character like every other, and
        // production then has to report it or the assertion below fails.
        //
        // So adding a string to `Identity` cannot reach the registry unguarded
        // without somebody deliberately writing a benign value here.
        //
        // **The values are not all newlines, and that is deliberate.** They were,
        // and narrowing production's `is_control()` to `== '\n'` then left the
        // whole suite green: the property was asserted at one of the sixty-five
        // characters `char::is_control` covers. Each class appears on at least
        // one field alone, so narrowing to any one of them fails here.
        let bad = Identity {
            role:            Role::Hand,
            staffed:         false,
            handle:          "pa\nja".into(),
            aliases:         vec!["a\x07b".into()],
            nickname:        Some("a\nb".into()),
            full_name:       Some("a\rb".into()),
            domain:          Some("a\tb".into()),
            git_name:        Some("a\0b".into()),
            git_email:       Some("a\x1bb".into()),
            committer_email: Some("a\rb".into()),
            committer_name:  Some("a\tb".into()),
            workspace:       Some("a\0b".into()),
            session:         Some("a\x1b[31mb".into()),
            repos:           vec!["a\nb".into()],
        };

        // **And every string in it is actually hostile.** `E0063` forces a new
        // field to be given a value; it cannot force that value to be one the
        // check should catch, so a benign one would satisfy the compiler and
        // leave the field unguarded. This closes that: serialise the entry and
        // require that every string in it carries a control character, so a
        // benign value fails here rather than silently passing below.
        //
        // `role` is the one exception, being a closed vocabulary that
        // serialises as a string and cannot carry one.
        {
            fn every_string(v: &toml::Value, at: &str, out: &mut Vec<(String, String)>) {
                match v {
                    toml::Value::String(s) => out.push((at.to_string(), s.clone())),
                    toml::Value::Table(t) => {
                        for (k, v) in t {
                            let next = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                            every_string(v, &next, out);
                        }
                    },
                    toml::Value::Array(a) => {
                        for v in a {
                            every_string(v, at, out);
                        }
                    },
                    _ => {},
                }
            }
            let mut all = Vec::new();
            every_string(
                &toml::Value::try_from(&bad).expect("an entry serialises"),
                "",
                &mut all,
            );
            for (field, value) in all {
                if field == "role" {
                    continue;
                }
                assert!(
                    value.chars().any(|c| c.is_control()),
                    "`{field}` was given a benign value in the hostile entry, so this \
                     test cannot tell whether the check covers it"
                );
            }
        }

        let mut ws = Workspace::parse(MINIMAL).expect("the minimal fixture parses");
        ws.org.insert("pike".into(), bad);

        let reported: std::collections::BTreeSet<String> =
            ws.unsafe_strings().into_iter().map(|(_, f)| f).collect();
        let want: std::collections::BTreeSet<String> =
            CORRUPTIBLE.iter().map(|s| s.to_string()).collect();

        assert_eq!(
            reported, want,
            "every free-form string carrying a control character must be reported, \
             and nothing else"
        );
    }

    #[test]
    fn each_class_of_control_character_is_reported_on_its_own() {
        // The fixture above distributes these across fields, which catches a
        // narrowing but states the law only by arrangement. This states it: one
        // field, one character, one assertion per class, so a later edit to the
        // fixture's distribution cannot quietly stop covering a class.
        //
        // `\x1b` is the one with a reason of its own. `stand.rs` interpolates
        // registry strings into terminal output, so an escape sequence in a
        // nickname writes colour codes, moves the cursor, or clears the line
        // somebody is reading a refusal on.
        for (name, c) in [
            ("newline", '\n'),
            ("carriage return", '\r'),
            ("tab", '\t'),
            ("nul", '\0'),
            ("escape", '\x1b'),
            ("vertical tab", '\x0b'),
            ("delete", '\x7f'),
        ] {
            let mut w = Workspace::parse(MINIMAL).unwrap();
            let mut id = Identity::new(Role::Hand, "pike");
            id.nickname = Some(format!("Pike{c}memory: project"));
            w.org.insert("pike".into(), id);

            assert_eq!(
                w.unsafe_strings(),
                vec![("pike".to_string(), "nickname".to_string())],
                "a {name} in a registry string must be reported"
            );
        }
    }

    #[test]
    fn a_clean_registry_reports_nothing_unsafe() {
        assert!(
            Workspace::parse(WITH_ORG)
                .unwrap()
                .unsafe_strings()
                .is_empty()
        );
    }
}
