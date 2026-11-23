//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const CATALOGUE: &str = r#"
[org.op]
role = "king"
handle = "op"
git_email = "op@example.invalid"

[org.onni]
role = "hand"
handle = "onni"
aliases = ["vouti"]
git_email = "onni.armas@example.invalid"

[org.paja]
role = "hand"
handle = "paja"
aliases = ["pajatso"]
git_email = "vaino.pajanen@example.invalid"
committer_email = "vaino.pajanen@example.invalid"

[org."säkä"]
role = "hand"
handle = "säkä"
git_email = "ilmari@example.invalid"

[org."os.si"]
role = "hand"
handle = "os.si"

[org.ikku]
role = "journeyman"
handle = "ikku"
git_email = "ikku@example.invalid"

[offices.coord]
holder = "onni"

[offices.tooling-track]
holder = "paja"

[offices.loru-coord]
counted = false

[offices.render-coord]
"#;

fn ws(text: &str) -> Workspace {
    Workspace::parse(text).unwrap()
}

#[test]
fn a_whole_catalogue_holds() {
    assert_eq!(check_catalogue(&ws(CATALOGUE)), vec![]);
}

#[test]
fn an_address_is_compared_lowercased_and_without_its_plus_tag() {
    assert_eq!(normalise(" Aarne+Review@IKIUNI.dev "), "aarne@ikiuni.dev");
    assert_eq!(normalise("aarne@ikiuni.dev"), "aarne@ikiuni.dev");
    // Only the local part carries a tag; a plus in the domain is not one.
    assert_eq!(normalise("a+b+c@x.invalid"), "a@x.invalid");
    assert_eq!(normalise("no-at-sign"), "no-at-sign");
}

#[test]
fn a_handle_may_carry_a_dot_and_any_script_but_never_leave_its_directory() {
    for good in ["paja", "os.si", "säkä", "pöllö", "1moremagicpot", "loru-coord", "a_b"] {
        assert_eq!(handle_fault(good), None, "`{good}` is a name");
    }
    for bad in ["", ".", "..", ".hidden", "a/b", "a\\b", "a b", "tab\there", "new\nline"] {
        assert!(handle_fault(bad).is_some(), "`{bad}` must be refused");
    }
}

#[test]
fn an_alias_one_entry_holds_cannot_be_another_entrys_handle_or_alias() {
    let text = format!(
        "{CATALOGUE}\n[org.vouti]\nrole = \"hand\"\nhandle = \"vouti\"\n\n[org.kide]\nrole = \
         \"journeyman\"\nhandle = \"kide\"\naliases = [\"pajatso\"]\n"
    );
    let faults = check_catalogue(&ws(&text));
    assert!(faults.contains(&CatalogueFault::NameTaken {
        name:   "vouti".into(),
        first:  "onni".into(),
        second: "vouti".into(),
    }));
    assert!(faults.contains(&CatalogueFault::NameTaken {
        name:   "pajatso".into(),
        first:  "kide".into(),
        second: "paja".into(),
    }));
}

#[test]
fn an_address_two_entries_answer_to_is_refused_even_through_case_and_a_tag() {
    let text = format!(
        "{CATALOGUE}\n[org.kide]\nrole = \"journeyman\"\nhandle = \"kide\"\ngit_email = \
         \"Ikku+kide@Example.invalid\"\n"
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::AddressShared {
            address: "ikku@example.invalid".into(),
            first:   "ikku".into(),
            second:  "kide".into(),
        }
    ]);
}

#[test]
fn an_entry_committing_as_its_own_author_shares_its_address_with_nobody() {
    // `paja` names one address twice, author and committer, which is one address.
    let faults = check_catalogue(&ws(CATALOGUE));
    assert!(
        !faults
            .iter()
            .any(|f| matches!(f, CatalogueFault::AddressShared { .. }))
    );
}

#[test]
fn an_office_naming_a_holder_the_catalogue_lacks_is_refused() {
    let text = CATALOGUE.replace(
        "[offices.render-coord]",
        "[offices.render-coord]\nholder = \"valtsu\"",
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::HolderMissing {
            office: "render-coord".into(),
            holder: "valtsu".into(),
        }
    ]);
}

#[test]
fn one_person_holding_two_offices_is_refused() {
    let text = CATALOGUE.replace(
        "[offices.render-coord]",
        "[offices.render-coord]\nholder = \"onni\"",
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::HoldsTwo {
            holder: "onni".into(),
            first:  "coord".into(),
            second: "render-coord".into(),
        }
    ]);
}

#[test]
fn an_office_named_like_a_handle_or_an_alias_is_refused() {
    let text = format!("{CATALOGUE}\n[offices.vouti]\n\n[offices.ikku]\n");
    let faults = check_catalogue(&ws(&text));
    assert_eq!(faults, vec![
        CatalogueFault::OfficeNamedLikeParticipant {
            office: "ikku".into(),
        },
        CatalogueFault::OfficeNamedLikeParticipant {
            office: "vouti".into(),
        },
    ]);
}

#[test]
fn a_bad_alias_is_named() {
    let text = CATALOGUE.replace("aliases = [\"vouti\"]", "aliases = [\"../vouti\"]");
    let faults = check_catalogue(&ws(&text));
    assert!(matches!(&faults[..], [CatalogueFault::BadName { name, .. }] if name == "../vouti"));
}

#[test]
fn the_king_is_no_agent_and_every_other_role_is() {
    assert!(!is_agent(Role::King));
    for r in [Role::Hand, Role::Expert, Role::General, Role::Journeyman] {
        assert!(is_agent(r));
    }
}
