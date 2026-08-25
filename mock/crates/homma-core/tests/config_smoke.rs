//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Smoke tests for `homma.toml` parsing.

use std::path::PathBuf;

use homma_core::{Config, ForgeKind};

#[test]
fn minimal_parse() {
    let src = r#"
[workspace]
name = "clause-dev"
"#;
    let config = Config::parse(src).unwrap();
    assert_eq!(config.workspace.name, "clause-dev");
    assert_eq!(config.workspace.path, PathBuf::from("."));
    assert_eq!(config.defaults.license, "MPL-2.0");
    assert_eq!(config.defaults.public_branch, "main");
    assert_eq!(config.defaults.working_branch, "dev");
    assert!(config.forges.is_empty());
    assert!(config.repos.is_empty());
}

#[test]
fn full_parse() {
    let src = r#"
[workspace]
name = "clause-dev"
path = "/Users/example/Dev/clause-dev"

[defaults]
license = "MPL-2.0"
public_branch = "main"
working_branch = "dev"

[forges.github]
kind = "github"
base_url = "https://github.com"
api_url = "https://api.github.com"
token_env = "GH_TOKEN"

[forges.codeberg]
kind = "forgejo"
base_url = "https://codeberg.org"
api_url = "https://codeberg.org/api/v1"
token_env = "CODEBERG_TOKEN"

"#;
    let config = Config::parse(src).unwrap();

    assert_eq!(config.workspace.name, "clause-dev");
    assert_eq!(
        config.workspace.path,
        PathBuf::from("/Users/example/Dev/clause-dev")
    );

    let gh = config.forge("github").expect("github forge present");
    assert_eq!(gh.kind, ForgeKind::Github);
    assert_eq!(gh.api_url, "https://api.github.com");
    assert_eq!(gh.token_env.as_deref(), Some("GH_TOKEN"));

    let cb = config.forge("codeberg").expect("codeberg forge present");
    assert_eq!(cb.kind, ForgeKind::Forgejo);

    // Nothing here says which repositories the workspace has, and a parse has
    // no filesystem to find out from.
    assert!(config.repos.is_empty());
}

#[test]
fn a_manifest_that_still_declares_repos_is_refused_rather_than_half_read() {
    // The table is gone, not optional. A manifest carrying one has an idea of
    // membership that homma no longer holds, and reading it as far as the
    // first unknown key would leave the operator with a workspace that looks
    // configured and is not.
    let src = r#"
[workspace]
name = "demo"

[repos.notko]
forge = "github"
owner = "orgrinrt"
local_path = "notko"
"#;
    let err = Config::parse(src).expect_err("a [repos] table must be refused");
    assert!(
        format!("{err}").contains("repos"),
        "the refusal does not name what is wrong: {err}"
    );
}

#[test]
fn unknown_field_rejected() {
    // `deny_unknown_fields` should reject typos to catch mis-spelled keys early.
    let src = r#"
[workspace]
name = "clause-dev"
nmae = "typo"
"#;
    assert!(Config::parse(src).is_err());
}

#[test]
fn from_path_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("homma.toml");
    std::fs::write(
        &path,
        r#"[workspace]
name = "demo"
path = "/tmp/demo"
"#,
    )
    .unwrap();

    let config = Config::from_path(&path).unwrap();
    assert_eq!(config.workspace.name, "demo");
    assert_eq!(config.workspace.path, PathBuf::from("/tmp/demo"));
}

#[test]
fn parse_via_fromstr() {
    use std::str::FromStr;
    let config = Config::from_str(
        r#"[workspace]
name = "demo"
"#,
    )
    .unwrap();
    assert_eq!(config.workspace.name, "demo");
}
