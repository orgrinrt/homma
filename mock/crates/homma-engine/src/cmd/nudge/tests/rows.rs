//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The row regen writes per event the nudges name.

use super::*;

fn rows_of(text: &str) -> Vec<crate::cmd::agent_shook::Row> {
    let table: AgentTable = toml::from_str(text).unwrap();
    crate::cmd::nudge::rows(&table.nudges)
}

#[test]
fn one_row_per_event_the_rows_name_with_their_tools_joined() {
    let text = format!(
        "{GREP}{ASKED}[[nudges]]\nname = \"r\"\nevent = \"Stop\"\nsays = \"x\"\nstrength = \
         \"inform\"\n"
    );
    let rows = rows_of(&text);
    let got: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| (r.event.as_str(), r.matcher.as_str()))
        .collect();
    assert_eq!(got, [
        ("PreToolUse", "Bash|Grep|AskUserQuestion"),
        ("Stop", "*")
    ]);
    for r in &rows {
        assert_eq!(r.name, "_nudges");
        assert_eq!(r.run, ["homma", "agent", "nudge", r.event.as_str()]);
        // from the workspace root, and never through the notice filter
        assert!(r.at_root && !r.filtered && r.paths.is_empty());
    }
}

#[test]
fn a_row_naming_no_tools_widens_the_matcher_to_every_tool() {
    let rows = rows_of(&format!(
        "{GREP}[[nudges]]\nname = \"any\"\nsays = \"x\"\nstrength = \"refuse\"\n"
    ));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].matcher, "*");
}

#[test]
fn no_nudges_no_row() {
    assert!(rows_of("").is_empty());
}
