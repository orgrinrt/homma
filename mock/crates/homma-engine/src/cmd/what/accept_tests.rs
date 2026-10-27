//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The acceptance set: queries a helper or the coordinator would type, run against the workspace's real
//! tools, scripts, skills and rules, each with the thing that must come first. Nothing here is a fixture
//! made to pass: every answer is a descriptor or a header in the tree the repository sits in.

use std::path::PathBuf;

use super::doc::Kind;
use super::{Answer, answer, gather};

/// The workspace around this repository: the nearest parent holding `.shared/scripts` and `tools`.
///
/// # Panics
///
/// Where there is none. The set is refused rather than skipped, since a skipped acceptance set is a
/// green one that checked nothing.
pub(super) fn workspace() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .find(|p| p.join(".shared/scripts").is_dir() && p.join("tools").is_dir())
        .unwrap_or_else(|| {
            panic!(
                "no workspace around {}: the acceptance set runs against the real tools, scripts, skills and \
                 rules of the workspace this repository is cloned into",
                here.display()
            )
        })
        .to_path_buf()
}

fn ask(q: &str) -> Answer {
    let g = gather(&workspace(), &[], true, &[]);
    answer(&g.docs, g.gaps, q, 5)
}

fn first(q: &str) -> (Kind, String, String) {
    let a = ask(q);
    let h = a
        .hits
        .first()
        .unwrap_or_else(|| panic!("`{q}` found nothing"));
    (h.kind, h.name.clone(), h.command.clone())
}

fn must_come_first(q: &str, kind: Kind, name: &str) {
    let (k, n, _) = first(q);
    assert_eq!(
        (k, n.as_str()),
        (kind, name),
        "`{q}` came first with the wrong thing"
    );
}

#[test]
fn land_a_branch_finds_the_script_that_lands_it() {
    must_come_first("land a branch", Kind::Script, "land-branch");
}

#[test]
fn which_helpers_are_done_finds_the_sessions_check_with_its_finished_verb() {
    must_come_first("which helpers are done", Kind::Script, "sessions");
    assert_eq!(
        first("which helpers are done").2,
        ".shared/scripts/remote/sessions finished"
    );
}

#[test]
fn free_disk_finds_disk_trim() {
    must_come_first("free disk", Kind::Tool, "disk-trim");
}

#[test]
fn a_misspelt_query_finds_the_same_tool() {
    must_come_first("fre dsik", Kind::Tool, "disk-trim");
}

#[test]
fn end_a_lost_cargo_run_finds_stray_runs_with_its_end_verb() {
    must_come_first("end a lost cargo run", Kind::Tool, "stray-runs");
    assert_eq!(first("end a lost cargo run").2, "homma stray-runs end");
}

#[test]
fn send_a_message_to_the_coordinator_finds_msg() {
    must_come_first("send a message to the coordinator", Kind::Tool, "msg");
}

#[test]
fn land_a_workspace_change_finds_the_workspace_lander() {
    must_come_first("land a workspace change", Kind::Script, "land-workspace-pr");
}

#[test]
fn work_on_a_mockspace_round_finds_the_skill_for_it() {
    must_come_first(
        "work on a mockspace round",
        Kind::Skill,
        "work-in-mockspace",
    );
}

#[test]
fn push_my_branch_and_say_it_is_ready_finds_ready() {
    must_come_first("push my branch and say it is ready", Kind::Script, "ready");
}

#[test]
fn is_this_red_test_red_on_dev_too_finds_red_check() {
    must_come_first("is this red test red on dev too", Kind::Script, "red-check");
}

#[test]
fn set_up_a_fresh_container_finds_the_bootstrap() {
    must_come_first("set up a fresh container", Kind::Script, "bootstrap");
}

#[test]
fn merge_dev_into_my_branch_finds_merge_dev() {
    must_come_first("merge dev into my branch", Kind::Script, "merge-dev");
}

#[test]
fn open_a_pull_request_finds_open_pr() {
    must_come_first("open a pull request", Kind::Script, "open-pr");
}

#[test]
fn the_filler_around_a_query_changes_nothing() {
    let plain = ask("free disk");
    let said = ask("homma what to use for freeing the disk");
    let name = |a: &Answer| a.hits.first().map(|h| h.name.clone());
    assert_eq!(name(&plain), name(&said));
}

/// Shown failing first: with the coverage rule taken out (`required` answering 1) the second query
/// answered with `documentation-writing`, `fetch`, `store-integrity`, `write-in-ops-voice` and `docs`,
/// each of which mentions `document`, and the test was red until the rule went back.
#[test]
fn a_query_about_nothing_the_tooling_does_returns_nothing() {
    for q in [
        "bake a sourdough loaf",
        "translate this document into french",
        "book a flight to helsinki",
    ] {
        let a = ask(q);
        assert!(
            a.hits.is_empty(),
            "`{q}` answered with {:?}",
            a.hits.iter().map(|h| &h.name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_query_of_only_filler_returns_nothing_and_says_why() {
    let a = ask("what is the way to do it");
    assert!(a.hits.is_empty() && a.terms.is_empty());
    let mut out = Vec::new();
    crate::output::HumanRender::render_human(&a, &mut out).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("filler"));
}

#[test]
fn the_best_answers_name_what_sits_beside_them() {
    let a = ask("free disk");
    let related: Vec<&str> = a.hits[0].related.iter().map(|r| r.name.as_str()).collect();
    assert!(
        related
            .iter()
            .any(|n| ["build-sweep", "stray-targets", "build-guard", "reap-worktrees"].contains(n)),
        "{related:?}"
    );
}

#[test]
fn every_answer_says_where_each_word_was_found() {
    let a = ask("free disk");
    let m = &a.hits[0].matched;
    assert!(
        m.iter().any(|m| m.word == "free" && m.how == "~syn~"),
        "{m:?}"
    );
    assert!(m.iter().any(|m| m.word == "disk" && m.how == "="), "{m:?}");
}

#[test]
fn the_footer_counts_the_scripts_with_no_header() {
    let a = ask("free disk");
    let g = a.gaps.iter().find(|g| g.source == "scripts");
    if let Some(g) = g {
        assert!(
            g.what
                .contains("scripts under .shared/scripts carry no header comment"),
            "{}",
            g.what
        );
        assert!(g.count > 0);
    }
}

#[test]
fn the_json_carries_the_query_the_terms_the_hits_and_the_gaps() {
    let v = serde_json::to_value(ask("free disk")).unwrap();
    for key in ["query", "terms", "read", "hits", "gaps"] {
        assert!(v.get(key).is_some(), "no `{key}` in {v}");
    }
    let h = &v["hits"][0];
    for key in ["kind", "name", "command", "summary", "score", "matched", "related"] {
        assert!(h.get(key).is_some(), "no `{key}` in {h}");
    }
    assert_eq!(h["kind"], "tool");
}

/// The one test that starts a process: `cargo mock tools` in the repositories of the workspace.
#[test]
fn lock_the_changelist_finds_the_mockspace_command() {
    let g = gather(&workspace(), &[Kind::Mock], false, &[]);
    assert!(
        g.docs.iter().any(|d| d.name == "lock"),
        "no mockspace command read; gaps: {:?}",
        g.gaps
    );
    let a = answer(&g.docs, g.gaps, "lock the active changelist", 3);
    assert_eq!(
        (a.hits[0].kind, a.hits[0].name.as_str()),
        (Kind::Mock, "lock")
    );
    assert_eq!(a.hits[0].command, "cargo mock lock");
}
