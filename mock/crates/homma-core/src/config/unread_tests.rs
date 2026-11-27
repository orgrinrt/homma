//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! A manifest entry this engine does not know: taken out, named, and refused
//! only by what reads it.

use crate::config::{Config, ConfigError};

const BASE: &str = "[workspace]\nname = \"w\"\n";

fn parsed(rest: &str) -> Result<Config, ConfigError> {
    Config::parse(&format!("homma_branch = \"dev\"\n{BASE}{rest}"))
}

#[test]
fn an_entry_of_the_top_level_or_of_agent_this_engine_does_not_know_is_kept_by_name() {
    let cfg = parsed(
        "[later]\nx = 1\n[[agent.later]]\na = 1\n[[agent.nudges]]\nname = \"n\"\nsays = \
         \"hi\"\nstrength = \"inform\"\n",
    )
    .expect("an entry this engine does not know leaves the rest read");
    assert_eq!(cfg.unread, ["agent.later", "later"]);
    let stray = Config::parse(&format!("stray = 2\n{BASE}")).expect("a bare key is an entry too");
    assert_eq!(stray.unread, ["stray"]);
    assert_eq!(cfg.workspace.name.to_string(), "w");
    assert_eq!(
        cfg.agent.nudges.iter().count(),
        1,
        "what is known is still read"
    );
    // Failing control: a manifest this engine knows has nothing unread.
    assert!(parsed("").unwrap().unread.is_empty());
}

#[test]
fn a_misspelt_key_inside_a_table_this_engine_knows_is_still_refused() {
    assert!(parsed("[defaults]\npublc_branch = \"main\"\n").is_err());
    // an unread entry beside it does not hide the misspelling
    assert!(parsed("[later]\nx = 1\n[defaults]\npublc_branch = \"main\"\n").is_err());
    // nor in a row of a table under `[agent]`
    let row = "[[agent.nudges]]\nname = \"n\"\nsays = \"hi\"\nstrenght = \"inform\"\n";
    assert!(parsed(row).is_err());
    assert!(parsed(&format!("[[agent.later]]\na = 1\n{row}")).is_err());
    // Failing control: the same keys spelt right are read.
    assert!(parsed("[defaults]\npublic_branch = \"main\"\n").is_ok());
}

#[test]
fn a_refusal_with_nothing_unread_is_returned_with_its_line() {
    let Err(ConfigError::Parse(e)) = parsed("[defaults]\npublc_branch = \"main\"\n") else {
        panic!("a misspelt key is refused as a parse error");
    };
    assert!(e.to_string().contains("line 5"), "{e}");
}

#[test]
fn the_words_name_the_entries_and_what_the_pin_asks() {
    let on_branch = parsed("[later]\nx = 1\n").unwrap();
    let said = on_branch.unread_note().expect("an unread entry is said");
    assert!(said.contains("`later`"), "{said}");
    assert!(said.contains("`reads`"), "{said}");
    assert!(said.contains("branch `dev`"), "{said}");
    assert!(said.contains("tip"), "{said}");

    let on_rev =
        Config::parse(&format!("homma_rev = \"abc1234\"\n{BASE}[later]\nx = 1\n")).unwrap();
    let said = on_rev.unread_note().unwrap();
    assert!(said.contains("`homma_rev = \"abc1234\"`"), "{said}");
    assert!(!said.contains("tip"), "{said}");

    let unpinned = Config::parse(&format!("{BASE}[later]\nx = 1\n")).unwrap();
    assert!(unpinned.unread_note().unwrap().contains("pins no engine"));

    // Failing control: nothing unread, nothing said.
    assert_eq!(parsed("").unwrap().unread_note(), None);
}

#[test]
fn only_an_entry_under_the_table_asked_for_refuses() {
    let top = parsed("[later]\nx = 1\n").unwrap();
    assert_eq!(top.unread_refusal("agent"), None);
    let under = parsed("[[agent.later]]\na = 1\n").unwrap();
    let why = under
        .unread_refusal("agent")
        .expect("an `[agent]` entry refuses there");
    assert!(why.contains("`agent.later`"), "{why}");
    // a name that only starts like the table is not under it
    let near = parsed("[agentish]\nx = 1\n").unwrap();
    assert_eq!(near.unread_refusal("agent"), None);
}
