//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A word after a tool's name becomes a verb, the tool's default command or a refusal that says what is
//! wrong, and the process it makes runs where the workspace is.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;

use super::descriptor::{Entry, discover};
use super::dispatch::*;

fn words(w: &[&str]) -> Vec<OsString> {
    w.iter().map(OsString::from).collect()
}

fn script(dir: &Path, rel: &str, mode: u32) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A workspace with `msg` (promoted, a default command and `show`), `quiet` (not promoted, one verb),
/// `remote` (a git backend) and `bare` (promoted, no default command).
fn workspace() -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    let tool = |name: &str, backend: &str, promote: bool, verbs: &[(&str, &str)]| {
        let d = ws.path().join("tools").join(name);
        let mut text = format!(
            "[tool]\nname=\"{name}\"\nsummary=\"{name} tool\"\nbackend=\"{backend}\"\npromote={promote}\n[tool.source]\npath={{path=\"tools/{name}\"}}\n"
        );
        for (v, run) in verbs {
            text.push_str(&format!(
                "[[tool.commands]]\nname=\"{v}\"\nsummary=\"s\"\nrun=\"{run}\"\n"
            ));
            script(&d, run, 0o755);
        }
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("tool.toml"), text).unwrap();
    };
    tool("msg", "local", true, &[
        ("msg", "msg"),
        ("show", "commands/show"),
    ]);
    tool("quiet", "local", false, &[("go", "go")]);
    tool("remote", "git", true, &[("remote", "remote")]);
    tool("bare", "local", true, &[("one", "one"), ("two", "two")]);
    ws
}

fn planned(ws: &tempfile::TempDir, name: &str, w: &[&str], asked: Asked) -> Result<Plan> {
    plan(&discover(ws.path()), name, &words(w), asked)
}

fn refusal(r: Result<Plan>) -> String {
    format!("{:#}", r.expect_err("it should be refused"))
}

#[test]
fn a_word_that_is_a_verb_runs_that_command_with_the_rest() {
    let ws = workspace();
    let p = planned(&ws, "msg", &["show", "abc", "--full"], Asked::AsSubcommand).unwrap();
    assert!(
        p.program.ends_with("tools/msg/commands/show"),
        "{:?}",
        p.program
    );
    assert_eq!(p.args, words(&["abc", "--full"]));
    assert_eq!(p.tool, "msg");
}

#[test]
fn a_word_that_is_no_verb_goes_to_the_command_named_like_the_tool_with_every_word() {
    let ws = workspace();
    let p = planned(
        &ws,
        "msg",
        &["kaski", "board", "--states"],
        Asked::AsSubcommand,
    )
    .unwrap();
    assert!(p.program.ends_with("tools/msg/msg"), "{:?}", p.program);
    assert_eq!(p.args, words(&["kaski", "board", "--states"]));
    let none = planned(&ws, "msg", &[], Asked::AsSubcommand).unwrap();
    assert!(none.args.is_empty() && none.program.ends_with("tools/msg/msg"));
}

#[test]
fn a_tool_with_no_default_refuses_an_unknown_verb_and_no_verb_listing_its_verbs() {
    let ws = workspace();
    let e = refusal(planned(&ws, "bare", &["three"], Asked::AsSubcommand));
    assert!(
        e.contains("no verb `three`") && e.contains("one, two"),
        "{e}"
    );
    let e = refusal(planned(&ws, "bare", &[], Asked::AsSubcommand));
    assert!(e.contains("wants a verb") && e.contains("one, two"), "{e}");
}

#[test]
fn a_tool_that_is_not_promoted_is_no_subcommand_but_runs_by_name() {
    let ws = workspace();
    let e = refusal(planned(&ws, "quiet", &["go"], Asked::AsSubcommand));
    assert!(
        e.contains("not promoted") && e.contains("homma tools run quiet"),
        "{e}"
    );
    assert!(planned(&ws, "quiet", &["go"], Asked::ByName).is_ok());
}

#[test]
fn a_backend_other_than_local_is_refused_naming_it() {
    let ws = workspace();
    let e = refusal(planned(&ws, "remote", &[], Asked::AsSubcommand));
    assert!(e.contains("`git`") && e.contains("local"), "{e}");
}

#[test]
fn a_broken_descriptor_is_refused_naming_the_tool_the_file_and_the_reason() {
    let ws = workspace();
    std::fs::write(
        ws.path().join("tools/bare/tool.toml"),
        "[tool]\nname=\"x\"\n",
    )
    .unwrap();
    let e = refusal(planned(&ws, "bare", &["one"], Asked::AsSubcommand));
    assert!(
        e.contains("`bare` is broken") && e.contains("tools/bare/tool.toml"),
        "{e}"
    );
    assert!(e.len() > 60, "the reason is missing: {e}");
}

#[test]
fn a_name_no_tool_has_is_refused_pointing_at_the_listings() {
    let ws = workspace();
    let e = refusal(planned(&ws, "nosuch", &[], Asked::AsSubcommand));
    assert!(
        e.contains("no command or tool named `nosuch`") && e.contains("homma tools"),
        "{e}"
    );
}

#[test]
fn a_command_whose_file_is_missing_a_directory_or_not_executable_is_refused() {
    let ws = workspace();
    std::fs::remove_file(ws.path().join("tools/msg/commands/show")).unwrap();
    assert!(refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand)).contains("not there"));
    std::fs::create_dir_all(ws.path().join("tools/msg/commands/show")).unwrap();
    assert!(refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand)).contains("not a file"));
    std::fs::remove_dir(ws.path().join("tools/msg/commands/show")).unwrap();
    script(&ws.path().join("tools/msg"), "commands/show", 0o644);
    assert!(
        refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand)).contains("not executable")
    );
}

#[test]
fn the_process_runs_from_the_workspace_root_told_where_it_is() {
    let ws = workspace();
    let p = planned(&ws, "msg", &["x"], Asked::AsSubcommand).unwrap();
    let c = command(&p, ws.path());
    assert_eq!(c.get_current_dir(), Some(ws.path()));
    let env: Vec<_> = c.get_envs().collect();
    assert!(
        env.iter()
            .any(|(k, v)| *k == "HOMMA_WORKSPACE" && *v == Some(ws.path().as_os_str())),
        "{env:?}"
    );
    assert_eq!(c.get_args().collect::<Vec<_>>(), ["x"]);
    assert!(matches!(discover(ws.path())[0], Entry::Tool(_)));
}
