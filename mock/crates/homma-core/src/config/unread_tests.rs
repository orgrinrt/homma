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

#[test]
fn an_entry_one_edit_from_a_name_this_engine_reads_is_refused_as_a_misspelling() {
    // a swap, a swap, and a letter dropped, each in its own table
    for (text, name) in [
        ("[defualts]\npublic_branch = \"x\"\n", "defualts"),
        ("homma_brnach = \"dev\"\n", "homma_brnach"),
        ("[[agent.nudge]]\nname = \"n\"\n", "nudge"),
    ] {
        let Err(ConfigError::Parse(e)) = Config::parse(&format!("{text}{BASE}")) else {
            panic!("{name} was taken as unread");
        };
        assert!(e.to_string().contains(name), "{name}: {e}");
    }
    // Failing control: two edits away is a name this engine does not know.
    let cfg = parsed("[defualtss]\nx = 1\n").unwrap();
    assert_eq!(cfg.unread, ["defualtss"]);
    // and a name under `[agent]` is compared with `[agent]`'s, not the top level's
    let cfg = parsed("[[agent.workspac]]\nx = 1\n").unwrap();
    assert_eq!(cfg.unread, ["agent.workspac"]);
}

#[test]
fn repos_and_local_are_refused_whatever_else_is_unread() {
    for table in ["repos", "local"] {
        let got = parsed(&format!("[later]\nx = 1\n[{table}.a]\nx = 1\n"));
        let Err(ConfigError::Parse(e)) = got else {
            panic!("[{table}] was taken as unread: {got:?}");
        };
        assert!(e.to_string().contains(table), "{table}: {e}");
        assert!(e.to_string().contains("line 6"), "{table}: {e}");
    }
    // Failing control: the unread entry alone is taken.
    assert_eq!(parsed("[later]\nx = 1\n").unwrap().unread, ["later"]);
}

#[test]
fn a_refusal_of_what_is_left_after_an_unread_entry_keeps_its_line() {
    let Err(ConfigError::Parse(e)) =
        parsed("[later]\nx = 1\n[defaults]\npublc_branch = \"main\"\n")
    else {
        panic!("a misspelt key beside an unread entry is refused as a parse error");
    };
    assert!(e.to_string().contains("publc_branch"), "{e}");
    assert!(e.to_string().contains("line 7"), "{e}");
}

#[test]
fn an_unread_entry_whose_value_runs_over_lines_is_still_taken() {
    let top = |rest: &str| Config::parse(&format!("homma_branch = \"dev\"\n{rest}{BASE}"));
    let cfg = top("later = [\n  1,\n  2,\n]\nlater_text = \"\"\"\nx\n\"\"\"\n").unwrap();
    assert_eq!(cfg.unread, ["later", "later_text"]);
    // and a dotted key at the top level is the entry it starts with
    let cfg = top("later.x = 1\n[agent]\nlater = 2\n").unwrap();
    assert_eq!(cfg.unread, ["agent.later", "later"]);
}

#[test]
fn an_unread_entry_the_line_reading_misses_is_taken_out_of_the_parsed_value() {
    // an inline `agent` table holds its entries on the key's own line, which the
    // blanking keeps, so the value is where the entry comes out
    let cfg = Config::parse(&format!(
        "homma_branch = \"dev\"\nagent = {{ later = 1 }}\n{BASE}"
    ))
    .unwrap();
    assert_eq!(cfg.unread, ["agent.later"]);
}
