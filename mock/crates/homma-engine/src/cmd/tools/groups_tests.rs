//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The group's word handed on, an unknown top level word reaching the dispatch, and the workspace root:
//! the parts of the tools module that every target has, and the refusal the others answer with.

use std::path::Path;

use clap::{CommandFactory, Parser};

use super::groups::{Handed, Parsed, asks_for_help, flag_value, groups, parse_from};
use super::{root, unix_only};
use crate::cli::{Cli, Command};

#[test]
fn what_a_target_without_renki_is_told_names_renki_and_unix() {
    let e = unix_only().to_string();
    assert!(e.contains("renki") && e.contains("unix only"), "{e}");
}

#[test]
fn an_unknown_top_level_word_reaches_the_dispatch_with_its_flags_untouched() {
    let cli =
        Cli::try_parse_from(["homma", "msg", "kaski", "board", "--states", "--json"]).unwrap();
    match cli.command {
        Command::External(w) => assert_eq!(w, ["msg", "kaski", "board", "--states", "--json"]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_root_is_the_manifests_directory_made_absolute() {
    assert_eq!(root(Path::new("/ws/homma.toml")), Path::new("/ws"));
    assert!(root(Path::new("homma.toml")).is_absolute());
}

/// The groups the design names. A command that gains verbs is a group the moment it does, and this list
/// is what says so out loud: the design's paragraph on the built-ins and this must agree.
const THE_DESIGNS_GROUPS: [&str; 14] = [
    "org",
    "repo",
    "forge",
    "agent",
    "docs",
    "local",
    "rules",
    "skills",
    "agents",
    "release",
    "milestone",
    "hook",
    "answers",
    "identity",
];

#[test]
fn the_groups_are_the_commands_that_take_verbs_and_tools_is_not_one() {
    assert_eq!(groups(), THE_DESIGNS_GROUPS);
    assert!(
        Cli::command()
            .find_subcommand("tools")
            .unwrap()
            .has_subcommands()
    );
}

#[test]
fn every_group_hands_an_unknown_verb_on_with_its_flags_and_the_global_options_given() {
    for g in groups() {
        let parsed =
            parse_from(["homma", "--dir", "/ws", "-vv", &g, "zzz-a-tool-verb", "--flag", "x"])
                .unwrap_or_else(|e| panic!("`{g}` does not take a verb of the tool's: {e}"));
        match parsed {
            Parsed::Handed(h) => {
                assert_eq!(
                    h,
                    Handed {
                        group:     g.clone(),
                        words:     ["zzz-a-tool-verb", "--flag", "x"].map(Into::into).to_vec(),
                        config:    None,
                        dir:       Some("/ws".into()),
                        verbosity: 2,
                    },
                    "{g}"
                );
            },
            Parsed::Cli(c) => panic!("`{g}` did not hand the word on: {c:?}"),
        }
    }
}

#[test]
fn a_group_s_own_verb_and_a_leaf_command_stay_the_built_ins() {
    for argv in [
        vec!["homma", "rules", "about", "x"],
        vec!["homma", "rules", "render"],
        vec!["homma", "repo", "config", "check"],
        vec!["homma", "status"],
        vec!["homma", "tools", "check"],
    ] {
        assert!(
            matches!(parse_from(argv.clone()), Ok(Parsed::Cli(_))),
            "{argv:?}"
        );
    }
}

#[test]
fn a_group_with_no_word_at_all_is_still_refused_by_clap() {
    assert!(parse_from(["homma", "rules"]).is_err());
}

/// What `parse_from` refuses with for these arguments, as the terminal would have printed it.
#[cfg(unix)]
fn printed(args: &[&str]) -> String {
    let e = parse_from(args.iter().copied()).expect_err("help is no command");
    assert_eq!(e.kind(), clap::error::ErrorKind::DisplayHelp, "{e}");
    e.to_string()
}

#[cfg(unix)]
fn workspace_with_tools() -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    let tool = |name: &str, body: &str| {
        let d = ws.path().join("tools").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("tool.toml"), body).unwrap();
    };
    tool(
        "checkup",
        "[tool]\nname=\"checkup\"\nsummary=\"checks the workspace over\"\nbackend=\"local\"\npromote=true\n[tool.source]\npath={path=\"tools/checkup\"}\n[[tool.commands]]\nname=\"checkup\"\nsummary=\"s\"\nrun=\"checkup\"\n",
    );
    tool("wrecked", "[tool]\nname=\"x\"\n");
    ws
}

#[cfg(unix)]
#[test]
fn help_lists_every_tool_with_its_summary_after_the_commands_and_names_a_broken_one() {
    let ws = workspace_with_tools();
    let dir = ws.path().to_str().unwrap();
    for argv in [["homma", "--dir", dir, "--help"], ["homma", "--dir", dir, "help"], [
        "homma", "-h", "--dir", dir,
    ]] {
        let text = printed(&argv);
        let tools = text
            .find("Tools:")
            .unwrap_or_else(|| panic!("{argv:?}: {text}"));
        assert!(tools > text.find("Commands:").unwrap(), "{text}");
        assert!(
            text[tools ..].contains("checkup  checks the workspace over"),
            "{text}"
        );
        assert!(text[tools ..].contains("wrecked  BROKEN:"), "{text}");
    }
    let by_file = ws.path().join("homma.toml");
    let text = printed(&["homma", &format!("--config={}", by_file.display()), "--help"]);
    assert!(
        text.contains("checkup  checks the workspace over"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn help_of_a_workspace_with_no_tools_or_no_workspace_is_the_help_it_was() {
    let empty = tempfile::tempdir().unwrap();
    let text = printed(&["homma", "--dir", empty.path().to_str().unwrap(), "--help"]);
    assert!(
        text.contains("Commands:") && !text.contains("Tools:"),
        "{text}"
    );
    let text = printed(&["homma", "--dir", "/no/such/workspace", "--help"]);
    assert!(
        text.contains("Commands:") && !text.contains("Tools:"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn the_help_of_one_command_does_not_carry_the_tools() {
    let ws = workspace_with_tools();
    let text = printed(&["homma", "--dir", ws.path().to_str().unwrap(), "status", "--help"]);
    assert!(!text.contains("Tools:"), "{text}");
}

fn argv(a: &[&str]) -> Vec<std::ffi::OsString> {
    a.iter().map(std::ffi::OsString::from).collect()
}

#[test]
fn no_arguments_at_all_ask_for_help_since_clap_prints_it_then() {
    assert!(asks_for_help(&argv(&["homma"])));
    assert!(asks_for_help(&argv(&["homma", "--help"])));
    assert!(asks_for_help(&argv(&["homma", "-h"])));
    assert!(asks_for_help(&argv(&["homma", "help"])));
    assert!(!asks_for_help(&argv(&["homma", "status"])));
    assert!(!asks_for_help(&argv(&["homma", "--dir", "w"])));
}

#[test]
fn a_flag_is_read_in_every_spelling_clap_accepts_and_the_last_one_wins() {
    let read = |a: &[&str]| flag_value(&argv(a), "--config", Some("-c"));
    let path = |p: &str| Some(std::path::PathBuf::from(p));
    assert_eq!(read(&["homma", "--config", "a"]), path("a"));
    assert_eq!(read(&["homma", "--config=a"]), path("a"));
    assert_eq!(read(&["homma", "-c", "a"]), path("a"));
    assert_eq!(read(&["homma", "-c=a"]), path("a"));
    assert_eq!(read(&["homma", "-ca"]), path("a"));
    assert_eq!(read(&["homma", "-c", "a", "--config=b", "-cc"]), path("c"));
    assert_eq!(read(&["homma", "--config", "a", "-c", "b"]), path("b"));
    assert_eq!(read(&["homma", "--dir", "a"]), None);
    assert_eq!(
        flag_value(&argv(&["homma", "--dir=a", "--dir", "b"]), "--dir", None),
        path("b")
    );
    assert_eq!(flag_value(&argv(&["homma", "-dx"]), "--dir", None), None);
}
