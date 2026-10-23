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

// What the tools stand on: renki's extension api, whose reading of a descriptor, whose finding of a tool
// and whose building of the process are the ones these hold the dispatch to.

/// A tool with a descriptor written out whole, and its files beside it.
fn raw_tool(ws: &Path, name: &str, descriptor: &str, files: &[(&str, &str)]) {
    let d = ws.join("tools").join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("tool.toml"), descriptor).unwrap();
    for (rel, body) in files {
        executable(&ws.join(rel), body);
    }
}

fn executable(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn local_descriptor(name: &str, source: &str, run: &str) -> String {
    format!(
        "[tool]\nname=\"{name}\"\nsummary=\"{name} summary\"\nbackend=\"local\"\npromote=true\n[tool.source]\npath={{path=\"{source}\"}}\n[[tool.commands]]\nname=\"{name}\"\nsummary=\"s\"\nrun=\"{run}\"\n"
    )
}

fn git_descriptor(name: &str, url: &str, rev: &str, run: &str) -> String {
    format!(
        "[tool]\nname=\"{name}\"\nsummary=\"{name} summary\"\nbackend=\"git\"\npromote=true\n[tool.source.git]\nurl=\"{url}\"\nrev=\"{rev}\"\n[[tool.commands]]\nname=\"{name}\"\nsummary=\"s\"\nrun=\"{run}\"\n"
    )
}

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn a_command_that_is_a_symlink_out_of_the_tool_is_refused_and_never_runs() {
    let ws = workspace();
    let marker = ws.path().join("outside/ran");
    executable(
        &ws.path().join("outside/evil"),
        &format!("touch {}", marker.display()),
    );
    raw_tool(
        ws.path(),
        "esc",
        &local_descriptor("esc", "tools/esc", "commands/go"),
        &[("tools/esc/commands/placeholder", "true")],
    );
    std::os::unix::fs::symlink(
        "../../../outside/evil",
        ws.path().join("tools/esc/commands/go"),
    )
    .unwrap();
    homma(ws.path())
        .args(["tools", "run", "esc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside the tool"));
    homma(ws.path()).arg("esc").assert().failure();
    assert!(!marker.exists(), "the file outside the tool ran");
}

#[test]
fn a_descriptor_renki_refuses_is_broken_here_and_names_what_it_refused() {
    let ws = workspace();
    for (name, descriptor) in [
        (
            "shortrev",
            git_descriptor(
                "shortrev",
                "https://example.com/t.git",
                "0123456789abcdef",
                "go",
            ),
        ),
        (
            "flagurl",
            git_descriptor("flagurl", "--upload-pack=x", REV, "go"),
        ),
        (
            "flagrev",
            git_descriptor(
                "flagrev",
                "https://example.com/t.git",
                "--upload-pack=x",
                "go",
            ),
        ),
        (
            "absrun",
            local_descriptor("absrun", "tools/absrun", "/bin/sh"),
        ),
        (
            "climbs",
            local_descriptor("climbs", "tools/climbs", "../other/go"),
        ),
        (
            "elsewhere",
            local_descriptor("elsewhere", "../outside", "go"),
        ),
    ] {
        raw_tool(ws.path(), name, &descriptor, &[]);
    }
    let out = homma(ws.path())
        .args(["tools", "check"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    for name in ["shortrev", "flagurl", "flagrev", "absrun", "climbs", "elsewhere"] {
        let line = out
            .lines()
            .find(|l| l.starts_with(name) && l.contains("BROKEN"))
            .unwrap_or_else(|| panic!("`{name}` is not listed as broken:\n{out}"));
        assert!(line.len() > 60, "no reason on the line: {line}");
    }
    homma(ws.path())
        .arg("shortrev")
        .assert()
        .failure()
        .stderr(predicate::str::contains("`shortrev` is broken"))
        .stderr(predicate::str::contains("0123456789abcdef"));
}

#[test]
fn a_run_that_starts_with_dot_slash_is_a_path_inside_the_tool() {
    let ws = workspace();
    raw_tool(
        ws.path(),
        "dotted",
        &local_descriptor("dotted", "tools/dotted", "./commands/go"),
        &[("tools/dotted/commands/go", "echo dotted:$*")],
    );
    homma(ws.path())
        .args(["dotted", "x"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dotted:x"));
}

#[test]
fn a_tool_is_run_from_where_its_source_names_not_from_the_directory_of_its_descriptor() {
    let ws = workspace();
    raw_tool(
        ws.path(),
        "moved",
        &local_descriptor("moved", "vendor/moved", "go"),
        &[("vendor/moved/go", "echo \"root=$HOMMA_TOOL_ROOT\"")],
    );
    let root = ws.path().join("vendor/moved");
    homma(ws.path())
        .arg("moved")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("root={}", root.display())));
    // and a source that is not there is refused saying where it looked
    std::fs::remove_dir_all(&root).unwrap();
    homma(ws.path())
        .arg("moved")
        .assert()
        .failure()
        .stderr(predicate::str::contains("vendor/moved"));
}

#[test]
fn the_process_is_told_where_the_tool_s_own_files_are() {
    let ws = workspace();
    raw_tool(
        ws.path(),
        "rooted",
        &local_descriptor("rooted", "tools/rooted", "go"),
        &[("tools/rooted/go", "echo \"root=$HOMMA_TOOL_ROOT\"")],
    );
    homma(ws.path())
        .arg("rooted")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "root={}",
            ws.path().join("tools/rooted").display()
        )));
}

#[test]
fn a_tool_with_the_git_backend_is_listed_without_a_fetch_and_run_from_the_shared_cache() {
    use renki::extension::{BUILTIN, Descriptor, Registry, cache_key};
    let ws = workspace();
    let cache = tempfile::tempdir().unwrap();
    let url = "https://example.com/pinned.git";
    let text = git_descriptor("pinned", url, REV, "go");
    raw_tool(ws.path(), "pinned", &text, &[]);

    // Listing reads the descriptor and nothing else: no cache directory is made or filled.
    let untouched = cache.path().join("never-made");
    homma(ws.path())
        .env("HOMMA_CACHE", &untouched)
        .arg("tools")
        .assert()
        .success()
        .stdout(predicate::str::contains("pinned"))
        .stdout(predicate::str::contains("BROKEN").not());
    assert!(!untouched.exists(), "listing touched the cache");

    // Once the revision is in the cache, a run uses that copy, where renki's key puts it.
    let d = Descriptor::parse(&text).unwrap();
    let key = cache_key(&d, Registry::new(BUILTIN).get("git").unwrap()).unwrap();
    let root = cache.path().join("tools").join(key);
    executable(
        &root.join("go"),
        "echo \"cached:$*\"; echo \"root=$HOMMA_TOOL_ROOT\"",
    );
    homma(ws.path())
        .env("HOMMA_CACHE", cache.path())
        .args(["pinned", "arg"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cached:arg"))
        .stdout(predicate::str::contains(format!("root={}", root.display())));
}

#[test]
fn a_backend_renki_has_no_row_for_is_refused_naming_the_ones_it_has() {
    let ws = workspace();
    raw_tool(
        ws.path(),
        "odd",
        &local_descriptor("odd", "tools/odd", "go").replace("backend=\"local\"", "backend=\"ftp\""),
        &[("tools/odd/go", "true")],
    );
    homma(ws.path())
        .arg("odd")
        .assert()
        .failure()
        .stderr(predicate::str::contains("`ftp`"))
        .stderr(predicate::str::contains("local"))
        .stderr(predicate::str::contains("git"));
}

#[test]
fn every_built_in_group_hands_a_verb_it_does_not_know_to_the_tool_of_its_name() {
    let groups = [
        "org", "repo", "forge", "agent", "docs", "local", "rules", "skills", "agents", "release",
        "hook",
    ];
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("homma.toml"), support::minimal_config_toml()).unwrap();
    for g in groups {
        raw_tool(
            ws.path(),
            g,
            &format!(
                "[tool]\nname=\"{g}\"\nsummary=\"s\"\nbackend=\"local\"\npromote=true\n[tool.source]\npath={{path=\"tools/{g}\"}}\n[[tool.commands]]\nname=\"zzz-tool-verb\"\nsummary=\"s\"\nrun=\"go\"\n"
            ),
            &[(
                &format!("tools/{g}/go"),
                &format!("echo \"reached:{g}:$*\""),
            )],
        );
    }
    for g in groups {
        homma(ws.path())
            .args([g, "zzz-tool-verb", "--flag"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("reached:{g}:--flag")));
    }
}
