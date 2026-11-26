//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
use super::*;
use crate::AgentTable;

fn parse(text: &str) -> Result<AgentTable, String> {
    toml::from_str::<AgentTable>(text).map_err(|e| e.to_string())
}

/// A row that loads, with `extra` lines added to it.
fn row(extra: &str) -> String {
    format!("[[nudges]]\nname = \"a\"\nsays = \"look first\"\nstrength = \"once\"\n{extra}\n")
}

#[test]
fn a_table_without_nudges_reads_as_none() {
    assert!(parse("").unwrap().nudges.is_empty());
}

#[test]
fn a_row_reads_with_its_defaults() {
    let t = parse(&row("")).unwrap();
    let n = t.nudges.iter().next().unwrap();
    assert_eq!(n.name(), "a");
    assert_eq!(n.event(), "PreToolUse");
    assert_eq!(n.matcher(), "*");
    assert_eq!(n.ttl(), Ttl::HOUR);
    assert_eq!(n.strength(), Strength::Once);
    assert!(n.takes_tool("Anything"));
    assert!(n.takes_text("anything at all"));
}

#[test]
fn tools_match_the_whole_tool_name_and_matches_any_part_of_the_text() {
    let t = parse(&row("tools = \"Bash|Grep\"\nmatches = '\\bgrep\\b'")).unwrap();
    let n = t.nudges.iter().next().unwrap();
    assert_eq!(n.matcher(), "Bash|Grep");
    assert!(n.takes_tool("Bash"));
    assert!(n.takes_tool("Grep"));
    assert!(!n.takes_tool("BashOutput"), "a tool name is matched whole");
    assert!(n.takes_text("ls | grep x"));
    assert!(!n.takes_text("ls | rgrep x"));
}

#[test]
fn a_key_the_row_does_not_know_refuses_the_table() {
    assert!(parse(&row("")).is_ok());
    let e = parse(&row("strenght = \"once\"")).unwrap_err();
    assert!(e.contains("strenght"), "{e}");
}

#[test]
fn a_key_agent_does_not_know_refuses_the_table() {
    assert!(parse("nudges = []").is_ok());
    let e = parse("reminders = []").unwrap_err();
    assert!(e.contains("reminders"), "{e}");
}

#[test]
fn an_expression_that_does_not_compile_is_refused_by_its_key() {
    assert!(parse(&row("matches = 'grep'")).is_ok());
    let e = parse(&row("matches = 'grep('")).unwrap_err();
    assert!(e.contains("`matches`") && e.contains("`a`"), "{e}");
    let e = parse(&row("tools = 'Bash('")).unwrap_err();
    assert!(e.contains("`tools`"), "{e}");
}

#[test]
fn a_stop_on_an_event_that_cannot_stop_a_call_is_refused() {
    assert!(parse(&row("event = \"PreToolUse\"")).is_ok());
    let e = parse(&row("event = \"SessionStart\"")).unwrap_err();
    assert!(e.contains("SessionStart") && e.contains("inform"), "{e}");
    let refuse =
        "[[nudges]]\nname = \"r\"\nsays = \"no\"\nstrength = \"refuse\"\nevent = \"Stop\"\n";
    assert!(parse(refuse).is_err());
    let inform =
        "[[nudges]]\nname = \"i\"\nsays = \"hi\"\nstrength = \"inform\"\nevent = \"Stop\"\n";
    assert!(parse(inform).is_ok());
}

#[test]
fn an_event_the_host_has_no_hook_for_is_refused() {
    let e = parse(&row("event = \"OnGrep\"")).unwrap_err();
    assert!(e.contains("OnGrep"), "{e}");
}

#[test]
fn a_program_outside_the_workspace_is_refused() {
    assert!(parse(&row("when = \"scripts/check\"")).is_ok());
    for bad in ["/usr/bin/check", "../check", "scripts/check --flag"] {
        let e = parse(&row(&format!("when = \"{bad}\""))).unwrap_err();
        assert!(e.contains("`when`"), "{bad}: {e}");
    }
    let e = parse(&row("answer = \"done\"\nevidence = \"../x\"")).unwrap_err();
    assert!(e.contains("`evidence`"), "{e}");
}

#[test]
fn a_key_its_strength_never_reads_is_refused() {
    assert!(parse(&row("answer = \"done\"\nevidence = \"scripts/x\"")).is_ok());
    let e = parse(&row("evidence = \"scripts/x\"")).unwrap_err();
    assert!(e.contains("`evidence`"), "{e}");
    let inform = "[[nudges]]\nname = \"i\"\nsays = \"hi\"\nstrength = \"inform\"\nanswer = \"x\"\n";
    let e = parse(inform).unwrap_err();
    assert!(e.contains("`answer`"), "{e}");
}

#[test]
fn a_marker_with_a_space_or_none_is_refused() {
    for bad in ["", "two words"] {
        assert!(
            parse(&row(&format!("answer = \"{bad}\""))).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn a_row_that_says_nothing_is_refused() {
    let e = parse("[[nudges]]\nname = \"a\"\nsays = \"  \"\nstrength = \"once\"\n").unwrap_err();
    assert!(e.contains("says nothing"), "{e}");
}

#[test]
fn a_name_that_is_not_a_slug_is_refused() {
    for bad in ["", "Grep", "a b", "a.b"] {
        let text = format!("[[nudges]]\nname = \"{bad}\"\nsays = \"x\"\nstrength = \"once\"\n");
        assert!(parse(&text).is_err(), "{bad:?}");
    }
}

#[test]
fn two_rows_of_one_name_are_refused() {
    let one = row("");
    assert!(parse(&one).is_ok());
    let e = parse(&format!("{one}{one}")).unwrap_err();
    assert!(e.contains("two nudges are named `a`"), "{e}");
}

#[test]
fn a_ttl_is_read_in_seconds_minutes_or_hours() {
    assert_eq!(Ttl::parse("90s").unwrap().seconds(), 90);
    assert_eq!(Ttl::parse("30m").unwrap().seconds(), 1800);
    assert_eq!(Ttl::parse("2h").unwrap().seconds(), 7200);
    for bad in ["", "h", "0m", "2d", "1.5h", "-1s", "2 h", "+3m", "99999999999999999999h"] {
        assert!(Ttl::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn appending_a_layer_adds_its_rows_after_and_refuses_a_name_already_read() {
    let base = parse(&row("")).unwrap().nudges;
    let other = parse(
        "[[nudges]]\nname = \"b\"\nsays = \"x\"\nstrength = \"refuse\"\n[[nudges]]\nname = \
         \"c\"\nsays = \"y\"\nstrength = \"inform\"\nevent = \"Stop\"\n",
    )
    .unwrap()
    .nudges;
    let mut merged = base.clone();
    merged.append(&other).unwrap();
    let names: Vec<&str> = merged.iter().map(Nudge::name).collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(merged.of_event("PreToolUse").count(), 2);
    assert_eq!(merged.events().into_iter().collect::<Vec<_>>(), [
        "PreToolUse",
        "Stop"
    ]);

    let mut again = merged.clone();
    let e = again.append(&base).unwrap_err();
    assert_eq!(e, InvalidNudge::Twice("a".into()));
    assert_eq!(again, merged, "a refused layer adds nothing");
}

#[test]
fn a_table_written_back_reads_the_same() {
    let t = parse(&row("tools = \"Bash\"\nanswer = \"done\"\nttl = \"5m\"")).unwrap();
    let text = toml::to_string(&t).unwrap();
    assert_eq!(parse(&text).unwrap(), t);
}

#[test]
fn a_brief_is_read_on_a_reminder_only() {
    let inform = "[[nudges]]\nname = \"b\"\nsays = \"bears on it:\"\nstrength = \"inform\"\nevent = \
                  \"UserPromptSubmit\"\nbrief = true\n";
    let t = parse(inform).unwrap();
    assert!(t.nudges.iter().next().unwrap().brief());
    assert!(
        !parse(&row(""))
            .unwrap()
            .nudges
            .iter()
            .next()
            .unwrap()
            .brief()
    );
    let e = parse(&row("brief = true")).unwrap_err();
    assert!(e.contains("`brief`"), "{e}");
    // false is what an absent key reads as, so it is read on any strength
    assert!(parse(&row("brief = false")).is_ok());
}
