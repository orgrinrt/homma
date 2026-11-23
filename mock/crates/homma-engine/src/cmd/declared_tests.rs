//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A row under `[agent.hooks]` becomes an entry of the dispatcher's, runs for
//! the calls it is narrowed to, and leaves the settings when it goes.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use homma_api::{AgentHook, AgentHooks};
use homma_core::scope::Scope;

use super::*;
use crate::cmd::aggregate::tests::{make_executable, test_root};
use crate::cmd::dispatch;

type Row<'a> = (&'a str, &'a str, &'a str, &'a [&'a str]);

fn rows(rows: &[Row<'_>]) -> AgentHooks {
    let mut m: BTreeMap<String, Vec<AgentHook>> = BTreeMap::new();
    for (event, matcher, run, repos) in rows {
        m.entry(event.to_string()).or_default().push(
            AgentHook::new(
                *matcher,
                *run,
                repos.iter().map(|s| s.to_string()).collect(),
            )
            .unwrap(),
        );
    }
    AgentHooks::new(m).unwrap()
}

fn table(r: &[Row<'_>]) -> AgentTable {
    AgentTable::new(rows(r), None, Vec::new()).unwrap()
}

/// A script under the workspace at `rel` that records its arguments in
/// `marker`, so whether it ran is observable.
fn plant_script(ws: &Path, rel: &str, marker: &Path) {
    let p = ws.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(
        &p,
        format!(
            "#!/usr/bin/env bash\ncat > /dev/null\nprintf '%s' \"$*\" > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    make_executable(&p);
}

fn repos() -> Vec<(String, String)> {
    vec![
        ("arvo".to_string(), "arvo".to_string()),
        ("kolli".to_string(), "./kolli".to_string()),
    ]
}

fn writing(file: &Path) -> String {
    format!(
        r#"{{"tool_name":"Bash","tool_input":{{"file_path":"{}"}}}}"#,
        file.display()
    )
}

/// The rows' entries written with the dispatcher, returning the dispatcher.
fn planted(ws: &Path, t: &AgentTable) -> std::path::PathBuf {
    let d = declared_entries(&test_root(ws), t, &repos(), &Scope::everything());
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    dispatch::tests::plant(ws, &d.entries, &[])
}

fn run(d: &Path, payload: &str) {
    let out = dispatch::tests::dispatch(d, &["PreToolUse"], payload, None, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_row_narrowed_to_a_repository_outside_the_scope_is_skipped_without_a_problem() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["elsewhere"])]),
        &repos(),
        &Scope::named(["alpha"]),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert!(d.entries.is_empty());
}

#[test]
fn a_row_naming_one_repository_inside_and_one_outside_the_scope_keeps_the_inside_one() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &[
            "arvo",
            "elsewhere",
        ])]),
        &repos(),
        &Scope::named(["arvo"]),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert_eq!(
        d.entries.len(),
        1,
        "the row stays for the repository in scope"
    );
    assert_eq!(d.entries[0].roots, vec!["arvo".to_string()]);
}

#[test]
fn a_row_whose_every_repository_is_outside_the_scope_is_dropped_and_not_widened() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &[
            "elsewhere",
            "other",
        ])]),
        &repos(),
        &Scope::named(["arvo"]),
    );
    assert!(
        d.entries.is_empty(),
        "a row with no names would apply to every call"
    );
    assert!(d.problems.is_empty());
}

#[test]
fn a_row_narrowed_to_an_unknown_name_is_still_a_problem_when_nothing_is_scoped() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["elsewhere"])]),
        &repos(),
        &Scope::everything(),
    );
    assert_eq!(d.problems.len(), 1, "{:?}", d.problems);
}

#[test]
fn a_row_becomes_an_entry_under_its_event_and_matcher_with_its_repositories() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "./scripts/guard", &["kolli"])]),
        &repos(),
        &Scope::everything(),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert_eq!(d.entries, vec![dispatch::Entry {
        event:      "PreToolUse".into(),
        matcher:    "Bash".into(),
        run:        "scripts/guard".into(),
        runner:     String::new(),
        args:       String::new(),
        // the manifest spells it `./kolli`
        roots:      vec!["kolli".into()],
        timeout:    homma_api::DEFAULT_TIMEOUT,
        background: false,
        from:       dispatch::FROM_ROWS.into(),
    }]);
}

#[test]
fn a_row_keeps_its_timeout_and_async_and_one_naming_none_takes_the_tables() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let mut m: BTreeMap<String, Vec<AgentHook>> = BTreeMap::new();
    m.insert("PostToolUse".into(), vec![
        AgentHook::new("Edit", "scripts/guard", vec![])
            .unwrap()
            .with_timeout(Some(5))
            .unwrap()
            .in_background(true),
        AgentHook::new("Write", "scripts/guard", vec![]).unwrap(),
    ]);
    let t = AgentTable::new(AgentHooks::new(m).unwrap(), Some(90), Vec::new()).unwrap();
    let d = declared_entries(&test_root(ws.path()), &t, &repos(), &Scope::everything());
    let got: Vec<(&str, u32, bool)> = d
        .entries
        .iter()
        .map(|e| (e.matcher.as_str(), e.timeout, e.background))
        .collect();
    assert_eq!(got, vec![("Edit", 5, true), ("Write", 90, false)]);
}

#[test]
fn a_row_naming_no_repositories_sees_every_call() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    let d = planted(
        ws.path(),
        &table(&[("PreToolUse", "", "scripts/guard", &[])]),
    );
    run(&d, &writing(&ws.path().join("elsewhere/x.rs")));
    assert!(marker.exists(), "a row with no repositories skipped a call");
}

#[test]
fn a_row_narrowed_to_repositories_runs_only_for_calls_landing_in_them() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    let d = planted(
        ws.path(),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["kolli"])]),
    );

    // A shell call whose session is inside the repository, which is the case
    // the row exists for: the manifest spells the path `./kolli`.
    let shell = format!(
        r#"{{"tool_name":"Bash","cwd":"{}","tool_input":{{"command":"cargo test"}}}}"#,
        ws.path().join("kolli/src").display()
    );
    run(&d, &shell);
    assert!(
        marker.exists(),
        "a call inside the named repository skipped the row"
    );

    fs::remove_file(&marker).unwrap();
    run(&d, &writing(&ws.path().join("arvo/x.rs")));
    assert!(!marker.exists(), "a call in another repository ran the row");
}

#[test]
fn a_row_sees_a_shell_call_at_the_root_that_names_its_repository_by_path() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    let d = planted(
        ws.path(),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["kolli"])]),
    );
    let at_root = |command: &str| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "cwd": ws.path().display().to_string(),
            "tool_input": { "command": command },
        })
        .to_string();
        run(&d, &payload);
        marker.exists()
    };
    assert!(at_root("cargo bench --manifest-path kolli/mock/Cargo.toml"));
    assert!(at_root("git -C kolli commit -m x"));
    // The control: the same calls naming the other repository.
    assert!(!at_root("cargo bench --manifest-path arvo/mock/Cargo.toml"));
    assert!(!at_root("git -C arvo commit -m x"));
}

#[test]
fn a_row_naming_nothing_it_can_run_is_reported_and_not_carried() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join("scripts")).unwrap();
    fs::write(ws.path().join("scripts/plain"), "#!/bin/sh\n").unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[
            ("PreToolUse", "Bash", "scripts/absent", &[]),
            ("PreToolUse", "Edit", "scripts/plain", &[]),
            ("Stop", "", "scripts/guard", &["nowhere"]),
        ]),
        &repos(),
        &Scope::everything(),
    );
    assert!(d.entries.is_empty(), "{:?}", d.entries);
    assert_eq!(d.problems.len(), 3, "{:?}", d.problems);
    assert!(d.problems[0].contains("scripts/absent") && d.problems[0].contains("no file"));
    assert!(d.problems[1].contains("scripts/plain") && d.problems[1].contains("not executable"));
    assert!(d.problems[2].contains("`nowhere`"));
}

#[test]
fn two_rows_running_one_script_under_one_event_are_an_entry_each() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = declared_entries(
        &test_root(ws.path()),
        &table(&[
            ("PreToolUse", "Bash", "scripts/guard", &[]),
            ("PreToolUse", "Edit", "scripts/guard", &["arvo"]),
        ]),
        &repos(),
        &Scope::everything(),
    );
    let got: Vec<(&str, &[String])> = d
        .entries
        .iter()
        .map(|e| (e.matcher.as_str(), e.roots.as_slice()))
        .collect();
    assert_eq!(got, vec![
        ("Bash", &[][..]),
        ("Edit", &["arvo".to_string()][..])
    ]);
}

#[test]
fn a_row_reaches_settings_through_the_dispatcher_and_leaves_it_when_it_goes() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    fs::create_dir_all(ws.path().join(".claude")).unwrap();
    fs::write(
        ws.path().join(".claude/settings.local.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/hand.sh"}]}]}}"#,
    )
    .unwrap();
    let root = test_root(ws.path());
    let commands = || -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(ws.path().join(".claude/settings.local.json")).unwrap(),
        )
        .unwrap();
        v["hooks"]["PreToolUse"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| e["hooks"][0]["command"].as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let regen = |t: &AgentTable| {
        let d = declared_entries(&root, t, &repos(), &Scope::everything());
        let regs = dispatch::install(&root, &d.entries, &[]).unwrap();
        aggregate::merge_settings(&root, &[], &regs).unwrap();
    };

    regen(&table(&[("PreToolUse", "Bash", "scripts/guard", &[])]));
    assert_eq!(commands(), vec![
        ".claude/hooks/hand.sh".to_string(),
        "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_dispatch.sh PreToolUse".to_string(),
    ]);

    regen(&table(&[]));
    assert_eq!(commands(), vec![".claude/hooks/hand.sh".to_string()]);
}

#[test]
fn a_registration_naming_a_file_homma_did_not_write_is_kept() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    fs::write(
        ws.path().join(".claude/hooks/_declared--Stop--mine.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    let body = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":".claude/hooks/_declared--Stop--mine.sh"}]}]}}"#;
    fs::write(ws.path().join(".claude/settings.json"), body).unwrap();
    aggregate::merge_settings(&test_root(ws.path()), &[], &[]).unwrap();
    assert_eq!(
        fs::read_to_string(ws.path().join(".claude/settings.json")).unwrap(),
        body
    );
}
