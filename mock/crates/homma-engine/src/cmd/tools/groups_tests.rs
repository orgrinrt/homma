//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The group's word handed on, an unknown top level word reaching the dispatch, and the workspace root:
//! the parts of the tools module that every target has, and the refusal the others answer with.

use std::path::Path;

use clap::{CommandFactory, Parser};

use super::groups::{Handed, Parsed, groups, parse_from};
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
const THE_DESIGNS_GROUPS: [&str; 13] = [
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
