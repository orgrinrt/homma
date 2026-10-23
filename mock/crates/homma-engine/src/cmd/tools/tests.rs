//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The listing says what is there, the merged and shadowed built-ins are named as such, and `check`
//! fails while a descriptor is broken.

use clap::Parser;

use super::groups::{Handed, Parsed, groups, parse_from};
use super::*;
use crate::cli::{Cli, Command};

fn tool_text(name: &str) -> String {
    format!(
        "[tool]\nname=\"{name}\"\nsummary=\"{name} summary\"\nbackend=\"local\"\npromote=true\n[tool.source]\npath={{path=\"tools/{name}\"}}\n[[tool.commands]]\nname=\"{name}\"\nsummary=\"s\"\nrun=\"{name}\"\n"
    )
}

fn workspace(tools: &[(&str, Option<String>)]) -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    for (name, text) in tools {
        let d = ws.path().join("tools").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("tool.toml"),
            text.clone().unwrap_or_else(|| tool_text(name)),
        )
        .unwrap();
    }
    ws
}

fn rendered(r: &ToolsReport) -> String {
    let mut out = Vec::new();
    r.render_human(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn the_listing_carries_the_summary_and_verbs_and_marks_a_tool_that_is_not_promoted() {
    let ws = workspace(&[
        ("msg", None),
        (
            "quiet",
            Some(tool_text("quiet").replace("promote=true", "promote=false")),
        ),
    ]);
    let text = rendered(&report(ws.path()));
    assert!(text.contains("msg    msg summary  [msg]\n"), "{text}");
    assert!(
        text.contains("quiet  quiet summary  [quiet]  (not promoted: `homma tools run quiet ...`)"),
        "{text}"
    );
}

#[test]
fn a_built_in_of_the_same_name_is_merged_for_a_group_in_the_list_and_shadowing_otherwise() {
    let ws = workspace(&[("rules", None), ("status", None), ("msg", None)]);
    let r = report(ws.path());
    let rel = |n: &str| r.tools.iter().find(|t| t.name == n).unwrap().builtin;
    assert_eq!(rel("rules"), Some("merged"));
    assert_eq!(rel("status"), Some("shadowed"));
    assert_eq!(rel("msg"), None);
    let text = rendered(&r);
    assert!(text.contains("merged with the built-in"), "{text}");
    assert!(
        text.contains("a built-in holds the name: `homma tools run status ...`"),
        "{text}"
    );
}

#[test]
fn a_broken_descriptor_is_listed_after_the_tools_and_no_tools_says_so() {
    let ws = workspace(&[("msg", None), ("bad", Some("[[[[".to_string()))]);
    let text = rendered(&report(ws.path()));
    assert!(
        text.lines().count() == 2 && text.lines().nth(1).unwrap().contains("BROKEN"),
        "{text}"
    );
    assert!(
        text.contains("bad/tool.toml") || text.contains("bad"),
        "{text}"
    );
    assert!(rendered(&report(tempfile::tempdir().unwrap().path())).starts_with("no tools"));
}

#[test]
fn check_is_a_failure_while_a_descriptor_is_broken_and_clean_otherwise() {
    let good = workspace(&[("msg", None)]);
    assert!(matches!(
        check(good.path(), OutputFormat::Human).unwrap(),
        Outcome::Ok
    ));
    let bad = workspace(&[("msg", None), ("bad", Some("[[[[".to_string()))]);
    assert!(matches!(
        check(bad.path(), OutputFormat::Human).unwrap(),
        Outcome::ReportedFailure
    ));
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
const THE_DESIGNS_GROUPS: [&str; 11] = [
    "org", "repo", "forge", "agent", "docs", "local", "rules", "skills", "agents", "release",
    "hook",
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
