//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A row under `[agent.hooks]` becomes a wrapper and a registration, and
//! leaves both when it goes.

use std::path::Path;

use homma_api::AgentHook;

use super::*;
use crate::cmd::aggregate::chains_tests::run_with_payload;
use crate::cmd::aggregate::tests::{make_executable, test_root};

type Row<'a> = (&'a str, &'a str, &'a str, &'a [&'a str]);

fn table(rows: &[Row<'_>]) -> AgentHooks {
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

fn wrapper(ws: &Path, name: &str) -> std::path::PathBuf {
    ws.join(".claude/hooks").join(name)
}

fn writing(file: &Path) -> String {
    format!(r#"{{"tool_input":{{"file_path":"{}"}}}}"#, file.display())
}

#[test]
fn a_row_becomes_a_marked_wrapper_registered_under_its_event_and_matcher() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = install_declared(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &[])]),
        &repos(),
    )
    .unwrap();
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert_eq!(d.entries, vec![HookEntry {
        event:   "PreToolUse".into(),
        matcher: "Bash".into(),
        command: "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_declared--PreToolUse--scripts_guard.sh"
            .into(),
    }]);
    assert!(is_declared_command(&d.entries[0].command));
    assert!(aggregate::carries_the_mark(&wrapper(
        ws.path(),
        "_declared--PreToolUse--scripts_guard.sh"
    )));
}

#[test]
fn a_row_naming_no_repositories_sees_every_call() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    install_declared(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "", "scripts/guard", &[])]),
        &repos(),
    )
    .unwrap();
    let w = wrapper(ws.path(), "_declared--PreToolUse--scripts_guard.sh");
    run_with_payload(&w, &writing(&ws.path().join("elsewhere/x.rs")), &["--x"]);
    assert_eq!(
        fs::read_to_string(&marker).unwrap(),
        "--x",
        "a row with no repositories skipped a call, or dropped its arguments"
    );
}

#[test]
fn a_row_narrowed_to_repositories_runs_only_for_calls_landing_in_them() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    install_declared(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["kolli"])]),
        &repos(),
    )
    .unwrap();
    let w = wrapper(ws.path(), "_declared--PreToolUse--scripts_guard.sh");

    // A shell call whose session is inside the repository, which is the case
    // the row exists for: the manifest spells the path `./kolli`.
    let shell = format!(
        r#"{{"cwd":"{}","tool_input":{{"command":"cargo test"}}}}"#,
        ws.path().join("kolli/src").display()
    );
    run_with_payload(&w, &shell, &[]);
    assert!(
        marker.exists(),
        "a call inside the named repository skipped the row"
    );

    fs::remove_file(&marker).unwrap();
    run_with_payload(&w, &writing(&ws.path().join("arvo/x.rs")), &[]);
    assert!(!marker.exists(), "a call in another repository ran the row");
}

#[test]
fn a_row_sees_a_shell_call_at_the_root_that_names_its_repository_by_path() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    plant_script(ws.path(), "scripts/guard", &marker);
    install_declared(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &["kolli"])]),
        &repos(),
    )
    .unwrap();
    let w = wrapper(ws.path(), "_declared--PreToolUse--scripts_guard.sh");
    let at_root = |command: &str| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({
            "cwd": ws.path().display().to_string(),
            "tool_input": { "command": command },
        })
        .to_string();
        run_with_payload(&w, &payload, &[]);
        marker.exists()
    };
    assert!(at_root("cargo bench --manifest-path kolli/mock/Cargo.toml"));
    assert!(at_root("git -C kolli commit -m x"));
    // The control: the same calls naming the other repository.
    assert!(!at_root("cargo bench --manifest-path arvo/mock/Cargo.toml"));
    assert!(!at_root("git -C arvo commit -m x"));
}

#[test]
fn a_row_naming_nothing_it_can_run_is_reported_and_not_registered() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join("scripts")).unwrap();
    fs::write(ws.path().join("scripts/plain"), "#!/bin/sh\n").unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = install_declared(
        &test_root(ws.path()),
        &table(&[
            ("PreToolUse", "Bash", "scripts/absent", &[]),
            ("PreToolUse", "Edit", "scripts/plain", &[]),
            ("Stop", "", "scripts/guard", &["nowhere"]),
        ]),
        &repos(),
    )
    .unwrap();
    assert!(d.entries.is_empty(), "{:?}", d.entries);
    assert_eq!(d.problems.len(), 3, "{:?}", d.problems);
    assert!(d.problems[0].contains("scripts/absent") && d.problems[0].contains("no file"));
    assert!(d.problems[1].contains("scripts/plain") && d.problems[1].contains("not executable"));
    assert!(d.problems[2].contains("`nowhere`"));
    let left: Vec<_> = fs::read_dir(ws.path().join(".claude/hooks"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        left.is_empty(),
        "wrappers written for rows that were refused: {left:?}"
    );
}

#[test]
fn two_rows_running_one_script_under_one_event_get_a_wrapper_each() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    let d = install_declared(
        &test_root(ws.path()),
        &table(&[
            ("PreToolUse", "Bash", "scripts/guard", &[]),
            ("PreToolUse", "Edit", "scripts/guard", &["arvo"]),
        ]),
        &repos(),
    )
    .unwrap();
    let got: Vec<(&str, &str)> = d
        .entries
        .iter()
        .map(|e| (e.matcher.as_str(), e.command.rsplit('/').next().unwrap()))
        .collect();
    assert_eq!(got, vec![
        ("Bash", "_declared--PreToolUse--scripts_guard.sh"),
        ("Edit", "_declared--PreToolUse--scripts_guard--2.sh"),
    ]);
    assert!(wrapper(ws.path(), "_declared--PreToolUse--scripts_guard.sh").exists());
    assert!(wrapper(ws.path(), "_declared--PreToolUse--scripts_guard--2.sh").exists());
}

#[test]
fn a_declared_wrapper_no_row_wants_is_removed_and_one_homma_did_not_write_is_not() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    let old = wrapper(ws.path(), "_declared--Stop--old.sh");
    fs::write(&old, declared_wrapper("Stop", "old", &[])).unwrap();
    let mine = wrapper(ws.path(), "_declared--Stop--mine.sh");
    fs::write(&mine, "#!/bin/sh\n").unwrap();

    install_declared(&test_root(ws.path()), &AgentHooks::default(), &repos()).unwrap();
    assert!(!old.exists(), "a wrapper whose row is gone survived");
    assert!(mine.exists(), "a file homma did not write was removed");
}

#[test]
fn a_file_homma_did_not_write_under_a_rows_name_is_refused() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    let theirs = wrapper(ws.path(), "_declared--PreToolUse--scripts_guard.sh");
    fs::write(&theirs, "#!/bin/sh\n# mine\n").unwrap();
    let d = install_declared(
        &test_root(ws.path()),
        &table(&[("PreToolUse", "Bash", "scripts/guard", &[])]),
        &repos(),
    )
    .unwrap();
    assert!(d.entries.is_empty());
    assert_eq!(d.problems.len(), 1, "{:?}", d.problems);
    assert!(d.problems[0].contains("not written by homma"));
    assert_eq!(fs::read_to_string(&theirs).unwrap(), "#!/bin/sh\n# mine\n");
}

#[test]
fn a_row_reaches_settings_and_leaves_it_when_it_goes() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard", &ws.path().join("fired"));
    fs::create_dir_all(ws.path().join(".claude")).unwrap();
    fs::write(
        ws.path().join(".claude/settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/hand.sh"}]}]}}"#,
    )
    .unwrap();
    let root = test_root(ws.path());
    let commands = || -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(ws.path().join(".claude/settings.json")).unwrap(),
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

    let d = install_declared(
        &root,
        &table(&[("PreToolUse", "Bash", "scripts/guard", &[])]),
        &repos(),
    )
    .unwrap();
    aggregate::merge_settings(&root, &[], &[], &d.entries, None).unwrap();
    assert_eq!(commands(), vec![
        ".claude/hooks/hand.sh".to_string(),
        "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_declared--PreToolUse--scripts_guard.sh"
            .to_string(),
    ]);

    let d = install_declared(&root, &AgentHooks::default(), &repos()).unwrap();
    aggregate::merge_settings(&root, &[], &[], &d.entries, None).unwrap();
    assert_eq!(commands(), vec![".claude/hooks/hand.sh".to_string()]);
}

#[test]
fn a_registration_naming_a_declared_file_homma_did_not_write_is_kept() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    fs::write(
        wrapper(ws.path(), "_declared--Stop--mine.sh"),
        "#!/bin/sh\n",
    )
    .unwrap();
    let body = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":".claude/hooks/_declared--Stop--mine.sh"}]}]}}"#;
    fs::write(ws.path().join(".claude/settings.json"), body).unwrap();
    aggregate::merge_settings(&test_root(ws.path()), &[], &[], &[], None).unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(ws.path().join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0]["command"],
        ".claude/hooks/_declared--Stop--mine.sh"
    );
}
