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
[org.oz]
role = "king"
handle = "oz"
git_email = "oz@example.invalid"

[org.abe]
role = "hand"
handle = "abe"
git_email = "arlen@example.invalid"

[org.inge]
role = "journeyman"
handle = "inge"
git_email = "inge@example.invalid"

[offices.chair]
holder = "abe"
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
    run(&cfg, &["identity", "match", "Arlen+review@example.invalid"])
        .success()
        .stdout("abe\thand\n");
}

#[test]
fn a_person_matches_no_agent_and_exits_one() {
    let (_d, cfg) = workspace("");
    run(&cfg, &["identity", "match", "oz@example.invalid"])
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
        "arlen@example.invalid\tabe\thand\ninge@example.invalid\tinge\tjourneyman\n"
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
        "\n[org.kent]\nrole = \"journeyman\"\nhandle = \"kent\"\ngit_email = \"inge+x@example.invalid\"\n",
    );
    run(&cfg, &["identity", "match", "inge@example.invalid"])
        .failure()
        .stderr(
            predicate::str::contains("inge@example.invalid").and(predicate::str::contains("kent")),
        );
    run(&cfg, &["org", "list"]).failure();
}
