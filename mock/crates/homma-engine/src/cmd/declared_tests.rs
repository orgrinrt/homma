//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A row under `[agent.hooks]` becomes a row of the manifest for claude-shook,
//! narrowed to its repositories by `paths`.

use std::collections::BTreeMap;
use std::path::Path;

use homma_api::{AgentHook, AgentHooks, AgentTable};
use homma_core::scope::Scope;

use super::*;
use crate::cmd::aggregate::tests::test_root;

type Spec<'a> = (&'a str, &'a str, &'a str, &'a [&'a str]);

fn table(rows: &[Spec<'_>]) -> AgentTable {
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
    AgentTable {
        hooks:  AgentHooks::new(m).unwrap(),
        nudges: homma_api::Nudges::default(),
    }
}

fn plant_script(ws: &Path, rel: &str) {
    homma_fixture::script(
        &ws.join(rel),
        "#!/usr/bin/env bash\ncat > /dev/null\n",
        0o755,
    );
}

fn repos() -> Vec<(String, String)> {
    vec![
        ("acre".to_string(), "acre".to_string()),
        ("krill".to_string(), "./krill".to_string()),
    ]
}

fn rows_of(ws: &Path, specs: &[Spec<'_>], scope: &Scope) -> Declared {
    declared_rows(&test_root(ws), &table(specs), &repos(), scope)
}

#[test]
fn a_row_becomes_a_manifest_row_under_its_event_and_matcher_narrowed_to_its_repositories() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[("PreToolUse", "Bash", "scripts/guard", &["acre", "krill"])],
        &Scope::everything(),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    let r = &d.rows[0];
    assert_eq!(
        (r.event.as_str(), r.matcher.as_str()),
        ("PreToolUse", "Bash")
    );
    assert_eq!(r.name, "_declared--scripts_guard");
    assert_eq!(r.run, ["{root}/scripts/guard"]);
    // `./krill` as the manifest wrote it, read as `krill`
    assert_eq!(r.paths, ["acre", "krill"]);
    assert!(r.filtered && !r.at_root);
}

#[test]
fn a_row_naming_no_repositories_sees_every_call() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[("Stop", "", "scripts/guard", &[])],
        &Scope::everything(),
    );
    assert!(d.rows[0].paths.is_empty());
}

#[test]
fn a_row_narrowed_to_a_repository_outside_the_scope_is_skipped_without_a_problem() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[("PreToolUse", "Bash", "scripts/guard", &["elsewhere"])],
        &Scope::named(["acre"]),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert!(
        d.rows.is_empty(),
        "a row with no names would apply to every call"
    );
}

#[test]
fn a_row_naming_one_repository_inside_and_one_outside_the_scope_keeps_the_inside_one() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[("PreToolUse", "Bash", "scripts/guard", &[
            "acre",
            "elsewhere",
        ])],
        &Scope::named(["acre"]),
    );
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    assert_eq!(d.rows[0].paths, ["acre"]);
}

#[test]
fn a_row_narrowed_to_an_unknown_name_is_still_a_problem_when_nothing_is_scoped() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[("PreToolUse", "Bash", "scripts/guard", &["nowhere"])],
        &Scope::everything(),
    );
    assert_eq!(d.problems.len(), 1, "{:?}", d.problems);
    assert!(d.rows.is_empty());
}

#[test]
fn a_row_naming_nothing_it_can_run_is_reported_and_not_registered() {
    let ws = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws.path().join("scripts")).unwrap();
    std::fs::write(ws.path().join("scripts/plain"), "#!/bin/sh\n").unwrap();
    let d = rows_of(
        ws.path(),
        &[
            ("PreToolUse", "Bash", "scripts/absent", &[]),
            ("PreToolUse", "Edit", "scripts/plain", &[]),
        ],
        &Scope::everything(),
    );
    assert!(d.rows.is_empty());
    assert_eq!(d.problems.len(), 2, "{:?}", d.problems);
    assert!(d.problems[0].contains("names no file"));
    assert!(d.problems[1].contains("not executable"));
}

#[test]
fn two_rows_running_one_script_under_one_event_are_told_apart_by_number() {
    let ws = tempfile::tempdir().unwrap();
    plant_script(ws.path(), "scripts/guard");
    let d = rows_of(
        ws.path(),
        &[
            ("PreToolUse", "Bash", "scripts/guard", &["acre"]),
            ("PreToolUse", "Edit", "scripts/guard", &["krill"]),
        ],
        &Scope::everything(),
    );
    let names: Vec<String> = crate::cmd::agent_shook::numbered(d.rows)
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, [
        "_declared--scripts_guard",
        "_declared--scripts_guard-2"
    ]);
}

#[test]
fn the_wrappers_an_older_homma_wrote_are_still_recognised_for_the_sweep() {
    assert!(is_declared_command(
        ".claude/hooks/_declared--PreToolUse--scripts_guard.sh"
    ));
    assert!(is_declared_command(
        "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_nudges--Stop.sh"
    ));
    assert!(!is_declared_command(".claude/hooks/acre--guard.sh"));
    assert!(!is_declared_command("scripts/_declared--x"));
}
