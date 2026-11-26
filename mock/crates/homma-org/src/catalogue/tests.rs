//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const CATALOGUE: &str = r#"
[org.oz]
role = "king"
handle = "oz"
git_email = "oz@example.invalid"

[org.olly]
role = "hand"
handle = "olly"
aliases = ["vesta"]
git_email = "olly.armas@example.invalid"

[org.pike]
role = "hand"
handle = "pike"
aliases = ["piketso"]
git_email = "vale.pikenen@example.invalid"
committer_email = "vale.pikenen@example.invalid"

[org."säkä"]
role = "hand"
handle = "säkä"
git_email = "ilmari@example.invalid"

[org."os.si"]
role = "hand"
handle = "os.si"

[org.inge]
role = "journeyman"
handle = "inge"
git_email = "inge@example.invalid"

[offices.chair]
holder = "olly"

[offices.workshop-desk]
holder = "pike"

[offices.loch-chair]
counted = false

[offices.render-chair]
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
    assert_eq!(normalise(" Arlen+Review@INLETS.dev "), "arlen@inlets.dev");
    assert_eq!(normalise("arlen@inlets.dev"), "arlen@inlets.dev");
    // The tag runs from the first plus of the local part; the domain is left as it is.
    assert_eq!(normalise("a+b+c@x.invalid"), "a@x.invalid");
    assert_eq!(normalise("a@b+c.invalid"), "a@b+c.invalid");
    assert_eq!(normalise("no-at-sign"), "no-at-sign");
}

#[test]
fn a_handle_may_carry_a_dot_and_any_script_but_never_leave_its_directory() {
    for good in ["pike", "os.si", "säkä", "pöllö", "1moremagicpot", "loch-chair", "a_b"] {
        assert_eq!(handle_fault(good), None, "`{good}` is a name");
    }
    for bad in ["", ".", "..", ".hidden", "a/b", "a\\b", "a b", "tab\there", "new\nline"] {
        assert!(handle_fault(bad).is_some(), "`{bad}` must be refused");
    }
}

#[test]
fn an_alias_one_entry_holds_cannot_be_another_entrys_handle_or_alias() {
    let text = format!(
        "{CATALOGUE}\n[org.vesta]\nrole = \"hand\"\nhandle = \"vesta\"\n\n[org.kent]\nrole = \
         \"journeyman\"\nhandle = \"kent\"\naliases = [\"piketso\"]\n"
    );
    let faults = check_catalogue(&ws(&text));
    assert!(faults.contains(&CatalogueFault::NameTaken {
        name:   "vesta".into(),
        first:  "olly".into(),
        second: "vesta".into(),
    }));
    assert!(faults.contains(&CatalogueFault::NameTaken {
        name:   "piketso".into(),
        first:  "kent".into(),
        second: "pike".into(),
    }));
}

#[test]
fn an_address_two_entries_answer_to_is_refused_even_through_case_and_a_tag() {
    let text = format!(
        "{CATALOGUE}\n[org.kent]\nrole = \"journeyman\"\nhandle = \"kent\"\ngit_email = \
         \"Inge+kent@Example.invalid\"\n"
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::AddressShared {
            address: "inge@example.invalid".into(),
            first:   "inge".into(),
            second:  "kent".into(),
        }
    ]);
}

#[test]
fn an_entry_committing_as_its_own_author_shares_its_address_with_nobody() {
    // `pike` names one address twice, author and committer, which is one address.
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
        "[offices.render-chair]",
        "[offices.render-chair]\nholder = \"vardon\"",
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::HolderMissing {
            office: "render-chair".into(),
            holder: "vardon".into(),
        }
    ]);
}

#[test]
fn one_person_holding_two_offices_is_refused() {
    let text = CATALOGUE.replace(
        "[offices.render-chair]",
        "[offices.render-chair]\nholder = \"olly\"",
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::HoldsTwo {
            holder: "olly".into(),
            first:  "chair".into(),
            second: "render-chair".into(),
        }
    ]);
}

#[test]
fn an_office_named_like_a_handle_or_an_alias_is_refused() {
    let text = format!("{CATALOGUE}\n[offices.vesta]\n\n[offices.inge]\n");
    let faults = check_catalogue(&ws(&text));
    assert_eq!(faults, vec![
        CatalogueFault::OfficeNamedLikeParticipant {
            office: "inge".into(),
        },
        CatalogueFault::OfficeNamedLikeParticipant {
            office: "vesta".into(),
        },
    ]);
}

#[test]
fn a_bad_alias_is_named() {
    let text = CATALOGUE.replace("aliases = [\"vesta\"]", "aliases = [\"../vesta\"]");
    let faults = check_catalogue(&ws(&text));
    assert!(matches!(&faults[..], [CatalogueFault::BadName { name, .. }] if name == "../vesta"));
}

#[test]
fn the_king_is_no_agent_and_every_other_role_is() {
    assert!(!is_agent(Role::King));
    for r in [Role::Hand, Role::Expert, Role::General, Role::Journeyman] {
        assert!(is_agent(r));
    }
}

#[test]
fn a_name_resolves_through_its_aliases_to_the_one_entry() {
    let w = ws(CATALOGUE);
    assert_eq!(resolve(&w, "pike").map(|i| i.handle.as_str()), Some("pike"));
    assert_eq!(
        resolve(&w, "piketso").map(|i| i.handle.as_str()),
        Some("pike")
    );
    assert_eq!(resolve(&w, "nobody"), None);
}

#[test]
fn an_office_held_by_alias_is_held_and_counts_against_the_person() {
    let text = CATALOGUE.replace("holder = \"olly\"", "holder = \"vesta\"");
    assert_eq!(check_catalogue(&ws(&text)), vec![]);
    let text = text.replace(
        "[offices.render-chair]",
        "[offices.render-chair]\nholder = \"olly\"",
    );
    assert_eq!(check_catalogue(&ws(&text)), vec![
        CatalogueFault::HoldsTwo {
            holder: "olly".into(),
            first:  "chair".into(),
            second: "render-chair".into(),
        }
    ]);
}
