//! Smoke tests for `homma.toml` parsing and the IntoMockspaceConfig bridge.

use std::path::PathBuf;

use homma_core::{Config, ForgeKind};
use mockspace_config::IntoMockspaceConfig;

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

[repos.notko]
forge = "github"
owner = "orgrinrt"
local_path = "notko"

[repos.arvo]
forge = "github"
owner = "orgrinrt"
local_path = "arvo"
public_branch = "main"
working_branch = "dev"
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

    let notko = config.repo("notko").expect("notko repo present");
    assert_eq!(notko.forge, "github");
    assert_eq!(notko.owner, "orgrinrt");
    assert_eq!(notko.local_path, PathBuf::from("notko"));
    assert!(notko.public_branch.is_none());

    let arvo = config.repo("arvo").expect("arvo repo present");
    assert_eq!(arvo.public_branch.as_deref(), Some("main"));
    assert_eq!(arvo.working_branch.as_deref(), Some("dev"));
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
fn into_mockspace_config_picks_name_and_path() {
    let src = r#"
[workspace]
name = "clause-dev"
path = "/some/path"
"#;
    let config = Config::parse(src).unwrap();
    let mockspace_cfg = config.into_mockspace_config().unwrap();

    assert_eq!(mockspace_cfg.project_name, "clause-dev");
    assert_eq!(mockspace_cfg.repo_root, PathBuf::from("/some/path"));
    // Unmapped fields default. Spot-check a few.
    assert_eq!(mockspace_cfg.mock_dir, PathBuf::from("mock"));
    assert_eq!(mockspace_cfg.crates_dir, PathBuf::from("mock/crates"));
    assert_eq!(mockspace_cfg.abi_version, 1);
}

#[test]
fn repo_branch_resolution() {
    let src = r#"
[workspace]
name = "demo"

[defaults]
public_branch = "trunk"
working_branch = "next"

[repos.bare]
forge = "github"
owner = "x"
local_path = "bare"

[repos.overridden]
forge = "github"
owner = "x"
local_path = "overridden"
public_branch = "release"
working_branch = "main"
"#;
    let config = Config::parse(src).unwrap();
    let bare = config.repo("bare").unwrap();
    assert_eq!(bare.resolved_public_branch(&config.defaults), "trunk");
    assert_eq!(bare.resolved_working_branch(&config.defaults), "next");

    let over = config.repo("overridden").unwrap();
    assert_eq!(over.resolved_public_branch(&config.defaults), "release");
    assert_eq!(over.resolved_working_branch(&config.defaults), "main");
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
