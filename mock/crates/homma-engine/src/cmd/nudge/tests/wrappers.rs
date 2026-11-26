//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The wrapper regen writes per event the rows name.

use super::*;
use crate::cmd::aggregate::tests::test_root;
use crate::cmd::declared::{install_declared, is_declared_command};

fn install(ws: &Path, rows: &str) -> crate::cmd::declared::Declared {
    let table: AgentTable = toml::from_str(rows).unwrap();
    install_declared(
        &test_root(ws),
        &table,
        &[],
        &homma_core::scope::Scope::everything(),
    )
    .unwrap()
}

#[test]
fn one_wrapper_per_event_the_rows_name_with_their_tools_joined() {
    let ws = tempfile::tempdir().unwrap();
    let rows = format!(
        "{GREP}{ASKED}[[nudges]]\nname = \"r\"\nevent = \"Stop\"\nsays = \"x\"\nstrength = \
         \"inform\"\n"
    );
    let d = install(ws.path(), &rows);
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    let got: Vec<(&str, &str)> = d
        .entries
        .iter()
        .map(|e| (e.event.as_str(), e.matcher.as_str()))
        .collect();
    assert_eq!(got, [
        ("PreToolUse", "Bash|Grep|AskUserQuestion"),
        ("Stop", "*")
    ]);
    assert!(d.entries.iter().all(|e| is_declared_command(&e.command)));
    let text =
        std::fs::read_to_string(ws.path().join(".claude/hooks/_nudges--PreToolUse.sh")).unwrap();
    assert!(
        text.contains("exec homma agent nudge 'PreToolUse'"),
        "{text}"
    );
    assert!(crate::cmd::aggregate::carries_the_mark(
        &ws.path().join(".claude/hooks/_nudges--Stop.sh")
    ));
}

#[test]
fn a_row_naming_no_tools_widens_the_matcher_to_every_tool() {
    let ws = tempfile::tempdir().unwrap();
    let d = install(
        ws.path(),
        &format!("{GREP}[[nudges]]\nname = \"any\"\nsays = \"x\"\nstrength = \"refuse\"\n"),
    );
    assert_eq!(d.entries[0].matcher, "*");
}

#[test]
fn no_rows_write_no_wrapper_and_sweep_the_ones_written_before() {
    let ws = tempfile::tempdir().unwrap();
    install(ws.path(), GREP);
    let w = ws.path().join(".claude/hooks/_nudges--PreToolUse.sh");
    assert!(w.is_file());
    let d = install(ws.path(), "");
    assert!(d.entries.is_empty());
    assert!(!w.exists());
}

#[test]
fn a_file_homma_did_not_write_is_not_written_over() {
    let ws = tempfile::tempdir().unwrap();
    let w = ws.path().join(".claude/hooks/_nudges--PreToolUse.sh");
    homma_fixture::script(&w, "#!/bin/sh\necho mine\n", 0o755);
    let d = install(ws.path(), GREP);
    assert_eq!(d.problems.len(), 1, "{:?}", d.problems);
    assert!(d.entries.is_empty());
    assert_eq!(
        std::fs::read_to_string(&w).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
    let d = install(ws.path(), "");
    assert!(d.problems.is_empty());
    assert!(w.exists(), "the sweep keeps it too");
}

#[test]
fn the_wrapper_runs_the_verb_from_the_workspace_with_the_payload() {
    let ws = tempfile::tempdir().unwrap();
    install(ws.path(), GREP);
    let bin = ws.path().join("bin");
    let seen = ws.path().join("seen");
    homma_fixture::script(
        &bin.join("homma"),
        &format!(
            "#!/usr/bin/env bash\n{{ pwd; echo \"$*\"; cat; }} > '{}'\n",
            seen.display()
        ),
        0o755,
    );
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut child =
        std::process::Command::new(ws.path().join(".claude/hooks/_nudges--PreToolUse.sh"))
            .current_dir("/")
            .env("PATH", path)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"tool_name\":\"Bash\"}")
        .unwrap();
    assert!(child.wait().unwrap().success());
    let text = std::fs::read_to_string(&seen).unwrap();
    let ws_real = ws.path().canonicalize().unwrap();
    assert_eq!(
        text,
        format!(
            "{}\nagent nudge PreToolUse\n{{\"tool_name\":\"Bash\"}}",
            ws_real.display()
        )
    );
}
