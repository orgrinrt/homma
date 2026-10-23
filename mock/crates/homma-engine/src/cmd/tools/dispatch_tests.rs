//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A word after a tool's name becomes a verb, the tool's default command or a refusal that says what is
//! wrong, and the process it makes is renki's: run where the workspace is, told where it and the tool
//! are, and never a file that leaves the tool.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use renki::extension::{BUILTIN, Descriptor, Registry, cache_key};

use super::Asked;
use super::descriptor::{Entry, discover};
use super::dispatch::*;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

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
/// `remote` (a git backend, promoted, no default) and `bare` (promoted, no default command).
fn workspace() -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    let tool = |name: &str, promote: bool, verbs: &[(&str, &str)]| {
        let d = ws.path().join("tools").join(name);
        let mut text = format!(
            "[tool]\nname=\"{name}\"\nsummary=\"{name} tool\"\nbackend=\"local\"\npromote={promote}\n[tool.source]\npath={{path=\"tools/{name}\"}}\n"
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
    tool("msg", true, &[("msg", "msg"), ("show", "commands/show")]);
    tool("quiet", false, &[("go", "go")]);
    tool("bare", true, &[("one", "one"), ("two", "two")]);
    ws
}

fn no_cache() -> Result<PathBuf, String> {
    Err("the cache was asked for by a tool that keeps none".to_string())
}

fn planned(ws: &tempfile::TempDir, name: &str, w: &[&str], asked: Asked) -> Result<Command> {
    plan(
        &discover(ws.path()),
        name,
        &words(w),
        asked,
        ws.path(),
        &no_cache,
    )
}

fn refusal(r: Result<Command>) -> String {
    format!("{:#}", r.expect_err("it should be refused"))
}

fn args(c: &Command) -> Vec<&std::ffi::OsStr> {
    c.get_args().collect()
}

#[test]
fn a_word_that_is_a_verb_runs_that_command_with_the_rest() {
    let ws = workspace();
    let c = planned(&ws, "msg", &["show", "abc", "--full"], Asked::AsSubcommand).unwrap();
    assert!(
        Path::new(c.get_program()).ends_with("tools/msg/commands/show"),
        "{:?}",
        c.get_program()
    );
    assert_eq!(args(&c), ["abc", "--full"]);
}

#[test]
fn a_word_that_is_no_verb_goes_to_the_command_named_like_the_tool_with_every_word() {
    let ws = workspace();
    let c = planned(
        &ws,
        "msg",
        &["kaski", "board", "--states"],
        Asked::AsSubcommand,
    )
    .unwrap();
    assert!(Path::new(c.get_program()).ends_with("tools/msg/msg"));
    assert_eq!(args(&c), ["kaski", "board", "--states"]);
    let none = planned(&ws, "msg", &[], Asked::AsSubcommand).unwrap();
    assert!(
        none.get_args().next().is_none()
            && Path::new(none.get_program()).ends_with("tools/msg/msg")
    );
}

#[test]
fn a_word_that_is_not_text_reaches_the_command_as_it_is() {
    use std::os::unix::ffi::OsStringExt;
    let ws = workspace();
    let odd = OsString::from_vec(vec![b'a', 0xFF, b'b']);
    let c = plan(
        &discover(ws.path()),
        "msg",
        std::slice::from_ref(&odd),
        Asked::AsSubcommand,
        ws.path(),
        &no_cache,
    )
    .unwrap();
    assert_eq!(args(&c), [odd.as_os_str()]);
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
    assert!(refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand)).contains("not there"));
    std::fs::remove_dir(ws.path().join("tools/msg/commands/show")).unwrap();
    script(&ws.path().join("tools/msg"), "commands/show", 0o644);
    assert!(
        refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand)).contains("not executable")
    );
}

#[test]
fn a_command_that_is_a_symlink_out_of_the_tool_is_refused() {
    let ws = workspace();
    script(ws.path(), "outside/evil", 0o755);
    let link = ws.path().join("tools/msg/commands/show");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("../../../outside/evil", &link).unwrap();
    let e = refusal(planned(&ws, "msg", &["show"], Asked::AsSubcommand));
    assert!(e.contains("outside the tool"), "{e}");
    // and one that points at another file inside the tool is the tool's own
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("../msg", &link).unwrap();
    assert!(planned(&ws, "msg", &["show"], Asked::AsSubcommand).is_ok());
}

#[test]
fn the_process_runs_from_the_workspace_root_told_where_it_and_the_tool_are() {
    let ws = workspace();
    let c = planned(&ws, "msg", &["x"], Asked::AsSubcommand).unwrap();
    assert_eq!(c.get_current_dir(), Some(ws.path()));
    let env: Vec<_> = c.get_envs().collect();
    let has = |k: &str, v: &Path| {
        env.iter()
            .any(|(ek, ev)| *ek == k && *ev == Some(v.as_os_str()))
    };
    assert!(has("HOMMA_WORKSPACE", ws.path()), "{env:?}");
    assert!(
        has("HOMMA_TOOL_ROOT", &ws.path().join("tools/msg")),
        "{env:?}"
    );
    assert!(matches!(discover(ws.path())[1], Entry::Tool(_)));
}

#[test]
fn a_tool_is_found_where_its_source_says_and_refused_saying_where_when_it_is_not_there() {
    let ws = workspace();
    let d = ws.path().join("tools/msg");
    let text = std::fs::read_to_string(d.join("tool.toml")).unwrap();
    std::fs::write(d.join("tool.toml"), text.replace("tools/msg", "vendor/msg")).unwrap();
    let e = refusal(planned(&ws, "msg", &["x"], Asked::AsSubcommand));
    assert!(e.contains("vendor/msg") && e.contains("not there"), "{e}");
    script(ws.path(), "vendor/msg/msg", 0o755);
    let c = planned(&ws, "msg", &["x"], Asked::AsSubcommand).unwrap();
    assert!(Path::new(c.get_program()).ends_with("vendor/msg/msg"));
}

fn git_tool(ws: &Path, backend: &str) -> Descriptor {
    let text = format!(
        "[tool]\nname=\"remote\"\nsummary=\"s\"\nbackend=\"{backend}\"\npromote=true\n[tool.source.git]\nurl=\"https://example.com/r.git\"\nrev=\"{REV}\"\n[[tool.commands]]\nname=\"remote\"\nsummary=\"s\"\nrun=\"go\"\n"
    );
    let d = ws.join("tools/remote");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("tool.toml"), &text).unwrap();
    Descriptor::parse(&text).unwrap()
}

#[test]
fn a_git_tool_is_run_from_its_place_in_the_cache_and_the_cache_is_asked_for_only_by_it() {
    let ws = workspace();
    let d = git_tool(ws.path(), "git");
    let cache = tempfile::tempdir().unwrap();
    let key = cache_key(&d, Registry::new(BUILTIN).get("git").unwrap()).unwrap();
    let root = cache.path().join("tools").join(key);
    script(&root, "go", 0o755);
    let asked = std::cell::Cell::new(0);
    let place = || {
        asked.set(asked.get() + 1);
        Ok(cache.path().to_path_buf())
    };
    let c = plan(
        &discover(ws.path()),
        "remote",
        &words(&["a"]),
        Asked::AsSubcommand,
        ws.path(),
        &place,
    )
    .unwrap();
    assert!(Path::new(c.get_program()).starts_with(&root));
    assert_eq!(asked.get(), 1);
    // a local tool never asks, so a machine with nowhere to keep a cache still runs it
    planned(&ws, "msg", &["x"], Asked::AsSubcommand).unwrap();
}

#[test]
fn a_cache_that_cannot_be_found_is_the_refusal_of_a_git_tool() {
    let ws = workspace();
    git_tool(ws.path(), "git");
    let e = refusal(planned(&ws, "remote", &[], Asked::AsSubcommand));
    assert!(e.contains("asked for by a tool"), "{e}");
}

#[test]
fn a_backend_renki_has_no_row_for_is_refused_with_the_ones_it_has() {
    let ws = workspace();
    git_tool(ws.path(), "ftp");
    let e = refusal(planned(&ws, "remote", &[], Asked::AsSubcommand));
    assert!(e.contains("`ftp`") && e.contains("local, git"), "{e}");
}
