//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma rules render` and `load`, chosen by the session's role and place.
//!
//! The binary runs on a scratch workspace with its own home directory, so the
//! role file and `CLAUSE_CLOUD` are the ones the test sets and never the
//! machine's.

mod support;

use std::path::Path;

use predicates::prelude::*;

/// One authored rule with the given extra meta lines.
fn rule(ws: &Path, name: &str, extra: &str) {
    let dir = ws.join(".shared/rules");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{name}.md.tmpl")),
        format!(
            "---\ntopics: [{name}, shared-topic]\nfires: \"x\"\nkind: reflex\n{extra}\n---\n\n\
             # {name}\n\nThe card.\n\n<!-- elaboration -->\n\nThe rest.\n"
        ),
    )
    .unwrap();
}

/// A workspace holding an untagged rule, one for each role, and one per place,
/// and a home directory beside it.
fn workspace() -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("homma.toml"), support::minimal_config_toml()).unwrap();
    rule(ws.path(), "for-all", "");
    rule(ws.path(), "for-coordinator", "roles: [coordinator]");
    rule(ws.path(), "for-helper", "roles: [helper]");
    rule(ws.path(), "for-local", "where: local");
    // Admitted everywhere but loaded only against matching files.
    rule(ws.path(), "gated", "paths: [\"**/*.rs\"]");
    std::fs::create_dir_all(ws.path().join("home")).unwrap();
    ws
}

/// Put the role file in the workspace's home.
fn role_file(ws: &Path, text: &str) {
    let path = ws.join("home").join(homma_api::session::ROLE_FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `homma rules <args>` with the workspace's home and the given cloud variable.
fn homma(ws: &Path, cloud: Option<&str>, args: &[&str]) -> assert_cmd::Command {
    let mut c = support::bin();
    c.arg("--dir").arg(ws).arg("rules").args(args);
    c.env("HOME", ws.join("home"));
    match cloud {
        Some(v) => c.env(homma_api::session::CLOUD_VAR, v),
        None => c.env_remove(homma_api::session::CLOUD_VAR),
    };
    c
}

/// The cards generated into the workspace, sorted.
fn generated(ws: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(ws.join(".claude/rules"))
        .map(|d| {
            d.map(|e| {
                e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .trim_end_matches(".md")
                    .to_string()
            })
            .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn a_helper_in_the_cloud_gets_its_own_cards_and_not_the_coordinators() {
    let ws = workspace();
    role_file(ws.path(), "helper\n");
    homma(ws.path(), Some("1"), &["render"]).assert().success();
    assert_eq!(generated(ws.path()), ["for-all", "for-helper", "gated"]);
}

#[test]
fn a_coordinator_in_the_cloud_gets_its_own_cards_and_not_the_helpers() {
    let ws = workspace();
    role_file(ws.path(), "coordinator\n");
    homma(ws.path(), Some("1"), &["render"]).assert().success();
    assert_eq!(generated(ws.path()), [
        "for-all",
        "for-coordinator",
        "gated"
    ]);
}

#[test]
fn a_session_with_no_role_file_gets_every_role_s_card() {
    // The control for the two above: without the file nothing is withheld, so
    // a lost file costs tokens and never a rule.
    let ws = workspace();
    homma(ws.path(), None, &["render"]).assert().success();
    assert_eq!(generated(ws.path()), [
        "for-all",
        "for-coordinator",
        "for-helper",
        "for-local",
        "gated"
    ]);
}

#[test]
fn the_place_comes_from_the_cloud_variable_being_exactly_one() {
    let ws = workspace();
    homma(ws.path(), Some("1"), &["render"]).assert().success();
    assert!(
        !generated(ws.path()).contains(&"for-local".to_string()),
        "cloud: no local card"
    );

    homma(ws.path(), Some("0"), &["render"]).assert().success();
    assert!(
        generated(ws.path()).contains(&"for-local".to_string()),
        "`0` is not the cloud"
    );
}

#[test]
fn an_option_overrides_the_role_file() {
    let ws = workspace();
    role_file(ws.path(), "helper\n");
    homma(ws.path(), Some("1"), &["render", "--role", "coordinator"])
        .assert()
        .success();
    assert_eq!(generated(ws.path()), [
        "for-all",
        "for-coordinator",
        "gated"
    ]);
}

#[test]
fn rendering_for_the_other_role_removes_the_first_roles_card() {
    let ws = workspace();
    homma(ws.path(), Some("1"), &["render", "--role", "coordinator"])
        .assert()
        .success();
    homma(ws.path(), Some("1"), &["render", "--role", "helper"])
        .assert()
        .success();
    assert_eq!(generated(ws.path()), ["for-all", "for-helper", "gated"]);
}

#[test]
fn a_misspelt_role_file_is_refused_by_name_and_writes_nothing() {
    let ws = workspace();
    role_file(ws.path(), "helpr\n");
    homma(ws.path(), Some("1"), &["render"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("helpr"))
        .stderr(predicate::str::contains("role"));
    assert!(
        generated(ws.path()).is_empty(),
        "no card is written for a session nobody could name"
    );
}

#[test]
fn discovery_still_finds_a_rule_the_session_does_not_load() {
    let ws = workspace();
    role_file(ws.path(), "helper\n");
    homma(ws.path(), Some("1"), &["render"]).assert().success();
    assert!(!generated(ws.path()).contains(&"for-coordinator".to_string()));
    homma(ws.path(), Some("1"), &["about", "for-coordinator"])
        .assert()
        .success()
        .stdout(predicate::str::contains("for-coordinator"));
}

#[test]
fn load_all_counts_what_each_kind_of_session_loads_and_matches_what_render_writes() {
    let ws = workspace();
    let json = |args: &[&str]| -> serde_json::Value {
        let o = support::bin()
            .arg("--dir")
            .arg(ws.path())
            .args(["--output", "json", "rules"])
            .args(args)
            .env("HOME", ws.path().join("home"))
            .env_remove(homma_api::session::CLOUD_VAR)
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        serde_json::from_slice(&o.stdout).unwrap()
    };

    let all = json(&["load", "--all"]);
    let rows = all["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        6,
        "no role, coordinator and helper, in each place"
    );
    let row = |role: &str, place: &str| {
        rows.iter()
            .find(|r| r["role"] == role && r["place"] == place)
            .unwrap()
            .clone()
    };
    assert_eq!(row("any", "local")["cards"], 4);
    assert_eq!(
        row("any", "cloud")["cards"],
        3,
        "the local-only card is not for the cloud"
    );
    assert_eq!(row("coordinator", "cloud")["cards"], 2);
    assert_eq!(row("helper", "cloud")["cards"], 2);
    assert_eq!(row("helper", "local")["cards"], 3);
    assert!(
        rows[0].get("names").is_none(),
        "the table over every session carries no names"
    );

    // The bytes reported, loaded and scoped together, are the bytes a render of
    // that session writes.
    for (role, place) in [("coordinator", "cloud"), ("helper", "local"), ("any", "cloud")] {
        let reported = row(role, place)["bytes"].as_u64().unwrap()
            + row(role, place)["scoped_bytes"].as_u64().unwrap();
        let mut render = vec!["render", "--place", place];
        if role != "any" {
            render.extend(["--role", role]);
        }
        // A role of `any` is the absence of the option, so it is rendered with
        // no role file in the home.
        homma(ws.path(), None, &render).assert().success();
        let written: u64 = std::fs::read_dir(ws.path().join(".claude/rules"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        assert_eq!(reported, written, "{role} {place}");
    }

    // One session names its cards, largest first.
    let one = json(&["load", "--role", "helper", "--place", "cloud"]);
    let names = one["rows"][0]["names"].as_array().unwrap();
    assert_eq!(
        names.len(),
        2,
        "the gated card is not among the loaded ones"
    );
    assert!(names.iter().all(|n| n["name"] != "gated"));
    let sizes: Vec<u64> = names.iter().map(|n| n["bytes"].as_u64().unwrap()).collect();
    assert!(
        sizes.windows(2).all(|w| w[0] >= w[1]),
        "largest first: {sizes:?}"
    );
    assert_eq!(one["rows"][0]["scoped_cards"], 1);
}

#[test]
fn a_gated_card_is_counted_apart_from_the_loaded_ones() {
    let ws = workspace();
    let o = support::bin()
        .arg("--dir")
        .arg(ws.path())
        .args(["--output", "json", "rules", "load", "--all"])
        .env("HOME", ws.path().join("home"))
        .env_remove(homma_api::session::CLOUD_VAR)
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    for r in doc["rows"].as_array().unwrap() {
        assert_eq!(r["scoped_cards"], 1, "{r}");
        assert!(r["scoped_bytes"].as_u64().unwrap() > 0, "{r}");
    }
    // The control: without the gated rule nothing is reported scoped.
    std::fs::remove_file(ws.path().join(".shared/rules/gated.md.tmpl")).unwrap();
    let o = support::bin()
        .arg("--dir")
        .arg(ws.path())
        .args(["--output", "json", "rules", "load", "--all"])
        .env("HOME", ws.path().join("home"))
        .env_remove(homma_api::session::CLOUD_VAR)
        .output()
        .unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        doc["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["scoped_cards"] == 0)
    );
}

#[test]
fn all_and_a_named_session_exclude_each_other() {
    let ws = workspace();
    homma(ws.path(), None, &["load", "--all", "--role", "helper"])
        .assert()
        .failure();
    homma(ws.path(), None, &["load", "--all", "--place", "cloud"])
        .assert()
        .failure();
}

#[test]
fn a_role_option_is_given_the_say_over_a_misspelt_role_file() {
    let ws = workspace();
    role_file(ws.path(), "helpr\n");
    homma(ws.path(), Some("1"), &["render", "--role", "helper"])
        .assert()
        .success();
    assert_eq!(generated(ws.path()), ["for-all", "for-helper", "gated"]);
}

#[test]
fn a_role_file_that_cannot_be_read_is_refused() {
    let ws = workspace();
    let path = ws.path().join("home").join(homma_api::session::ROLE_FILE);
    std::fs::create_dir_all(&path).unwrap();
    homma(ws.path(), Some("1"), &["render"]).assert().failure();
}
