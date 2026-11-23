//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The catalogue through the binary: `identity match` answers an address with
//! its agent or exits 1, `identity index` and `agent regen` write the index a
//! hook reads, and a catalogue that breaks a constraint is refused by name.

mod support;

use std::fs;
use std::path::Path;

use predicates::prelude::*;

const CATALOGUE: &str = r#"
[org.op]
role = "king"
handle = "op"
git_email = "op@example.invalid"

[org.aku]
role = "hand"
handle = "aku"
git_email = "aarne@example.invalid"

[org.ikku]
role = "journeyman"
handle = "ikku"
git_email = "ikku@example.invalid"

[offices.coord]
holder = "aku"
"#;

fn workspace(extra: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("homma.toml");
    fs::write(
        &cfg,
        format!("{}{CATALOGUE}{extra}", support::minimal_config_toml()),
    )
    .unwrap();
    (dir, cfg)
}

fn run(cfg: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    support::bin()
        .args(["--config", cfg.to_str().unwrap()])
        .args(args)
        .assert()
}

#[test]
fn a_tagged_sub_agent_address_matches_its_launcher() {
    let (_d, cfg) = workspace("");
    run(&cfg, &["identity", "match", "Aarne+review@example.invalid"])
        .success()
        .stdout("aku\thand\n");
}

#[test]
fn a_person_matches_no_agent_and_exits_one() {
    let (_d, cfg) = workspace("");
    run(&cfg, &["identity", "match", "op@example.invalid"])
        .code(1)
        .stdout("");
    run(&cfg, &["identity", "match", "stranger@example.invalid"]).code(1);
}

#[test]
fn the_index_is_written_under_the_paths_key() {
    let (d, cfg) = workspace("");
    run(&cfg, &["identity", "index"]).success();
    let index = fs::read_to_string(d.path().join(".shared/.index/identities.tsv")).unwrap();
    assert_eq!(
        index,
        "aarne@example.invalid\taku\thand\nikku@example.invalid\tikku\tjourneyman\n"
    );
}

#[test]
fn a_regen_writes_the_index_too() {
    let (d, cfg) = workspace("");
    run(&cfg, &[
        "agent",
        "regen",
        "--skip-cargo-mock",
        "--skip-configs",
        "--skip-aggregate",
        "--skip-surfaces",
    ]);
    assert!(d.path().join(".shared/.index/identities.tsv").is_file());
}

#[test]
fn a_catalogue_with_a_shared_address_is_refused_naming_it() {
    let (_d, cfg) = workspace(
        "\n[org.kide]\nrole = \"journeyman\"\nhandle = \"kide\"\ngit_email = \"ikku+x@example.invalid\"\n",
    );
    run(&cfg, &["identity", "match", "ikku@example.invalid"])
        .failure()
        .stderr(
            predicate::str::contains("ikku@example.invalid").and(predicate::str::contains("kide")),
        );
    run(&cfg, &["org", "list"]).failure();
}
