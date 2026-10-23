//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The content repository's tools as subcommands, through the binary.
//!
//! The tools here are shell scripts that print what they were given, so what is asserted is what the
//! process that replaced homma saw: its words, its directory, its environment, its input and its exit
//! status.

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use predicates::prelude::*;
use support::bin;

fn tool(ws: &Path, name: &str, promote: bool, verbs: &[(&str, &str)]) {
    let d = ws.join("tools").join(name);
    std::fs::create_dir_all(&d).unwrap();
    let mut text = format!(
        "[tool]\nname=\"{name}\"\nsummary=\"{name} summary\"\nbackend=\"local\"\npromote={promote}\n[tool.source]\npath={{path=\"tools/{name}\"}}\n"
    );
    for (verb, body) in verbs {
        let run = format!("commands/{verb}");
        text.push_str(&format!(
            "[[tool.commands]]\nname=\"{verb}\"\nsummary=\"s\"\nrun=\"{run}\"\n"
        ));
        let p = d.join(&run);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(d.join("tool.toml"), text).unwrap();
}

fn workspace() -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("homma.toml"), support::minimal_config_toml()).unwrap();
    tool(ws.path(), "msg", true, &[(
        "msg",
        "echo \"msg:$*\"; pwd; echo \"ws=$HOMMA_WORKSPACE\"",
    )]);
    tool(ws.path(), "rules", true, &[
        ("show", "echo \"show:$*\""),
        ("list", "echo list"),
    ]);
    tool(ws.path(), "status", true, &[(
        "status",
        "echo the-tools-status",
    )]);
    tool(ws.path(), "quiet", false, &[("go", "echo went:$*")]);
    tool(ws.path(), "boom", true, &[("boom", "cat; exit 7")]);
    ws
}

fn homma(ws: &Path) -> assert_cmd::Command {
    let mut c = bin();
    c.arg("--dir").arg(ws);
    c
}

#[test]
fn a_promoted_tool_is_a_subcommand_that_gets_the_words_after_its_name() {
    let ws = workspace();
    let real = std::fs::canonicalize(ws.path()).unwrap();
    homma(ws.path())
        .args(["msg", "kaski", "board", "--states"])
        .assert()
        .success()
        .stdout(predicate::str::contains("msg:kaski board --states"))
        .stdout(predicate::str::contains(real.to_str().unwrap()))
        .stdout(predicate::str::contains(format!(
            "ws={}",
            ws.path().display()
        )));
}

#[test]
fn the_tools_exit_status_and_standard_input_are_hommas_own() {
    let ws = workspace();
    homma(ws.path())
        .arg("boom")
        .write_stdin("piped in\n")
        .assert()
        .code(7)
        .stdout(predicate::eq("piped in\n"));
}

#[test]
fn a_built_in_group_and_the_tool_of_its_name_answer_together() {
    let ws = workspace();
    homma(ws.path())
        .args(["rules", "show", "some-rule"])
        .assert()
        .success()
        .stdout(predicate::str::contains("show:some-rule"));
    // the built-in's own verbs still reach the built-in
    homma(ws.path())
        .args(["rules", "about", "anything"])
        .assert()
        .stdout(predicate::str::contains("show:").not());
}

#[test]
fn a_word_neither_the_group_nor_the_tool_knows_names_both_sets() {
    let ws = workspace();
    homma(ws.path())
        .args(["rules", "nosuch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no verb `nosuch`"))
        .stderr(predicate::str::contains("about"))
        .stderr(predicate::str::contains("render"))
        .stderr(predicate::str::contains("show"));
}

#[test]
fn a_built_in_wins_over_a_tool_of_its_name_and_the_tool_runs_by_name() {
    let ws = workspace();
    homma(ws.path())
        .arg("status")
        .assert()
        .stdout(predicate::str::contains("the-tools-status").not());
    homma(ws.path())
        .args(["tools", "run", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("the-tools-status"));
}

#[test]
fn a_tool_that_is_not_promoted_is_refused_as_a_subcommand_and_runs_by_name() {
    let ws = workspace();
    homma(ws.path())
        .args(["quiet", "go"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not promoted"));
    homma(ws.path())
        .args(["tools", "run", "quiet", "go", "--flag"])
        .assert()
        .success()
        .stdout(predicate::str::contains("went:--flag"));
}

#[test]
fn an_unknown_name_is_refused_pointing_at_the_listing_instead_of_clap_s_words() {
    let ws = workspace();
    homma(ws.path())
        .arg("nosuchthing")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no command or tool named `nosuchthing`",
        ))
        .stderr(predicate::str::contains("homma tools"));
}

#[test]
fn the_listing_names_every_tool_and_check_fails_only_while_one_is_broken() {
    let ws = workspace();
    homma(ws.path())
        .arg("tools")
        .assert()
        .success()
        .stdout(predicate::str::contains("msg"))
        .stdout(predicate::str::contains("boom"))
        .stdout(predicate::str::contains("merged with the built-in"));
    homma(ws.path())
        .args(["tools", "check"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    std::fs::write(ws.path().join("tools/boom/tool.toml"), "garbage [").unwrap();
    homma(ws.path())
        .args(["tools", "check"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("boom"))
        .stdout(predicate::str::contains("msg summary").not())
        .stdout(predicate::str::contains("BROKEN"));
    // and a command of the broken tool says which tool and why, not that it is unknown
    homma(ws.path())
        .arg("boom")
        .assert()
        .failure()
        .stderr(predicate::str::contains("`boom` is broken"))
        .stderr(predicate::str::contains("tools/boom/tool.toml"));
}

#[test]
fn the_listing_is_a_json_document_under_output_json() {
    let ws = workspace();
    let out = homma(ws.path())
        .args(["--output", "json", "tools"])
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one JSON document");
    let names: Vec<&str> = doc["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["boom", "msg", "quiet", "rules", "status"]);
    assert_eq!(doc["tools"][0]["promote"], true);
    assert!(doc["broken"].as_array().unwrap().is_empty());
}
