use std::path::Path;

use super::{Config, ConfigError, Retired};

const BASE: &str = "[workspace]\nname = \"w\"\n";

#[test]
fn a_manifest_still_carrying_the_git_hooks_table_parses_and_names_it() {
    let cfg = Config::parse(&format!(
        "{BASE}\n[[hooks.pre-commit]]\nrun = \"vibecheck check {{paths}}\"\npaths = [\"*.md\"]\n"
    ))
    .unwrap();
    assert_eq!(cfg.retired, vec![Retired {
        table: "hooks".into(),
        file:  "homma.toml".into(),
    }]);
    assert_eq!(cfg.workspace.name.to_string(), "w", "the rest is read");
}

#[test]
fn a_manifest_without_it_names_nothing() {
    let cfg = Config::parse(BASE).unwrap();
    assert!(cfg.retired.is_empty());
}

#[test]
fn a_misspelt_key_beside_a_retired_table_is_still_refused() {
    // taking the retired table out does not make the rest lenient
    let got = Config::parse(&format!(
        "{BASE}\n[defaults]\npublc_branch = \"main\"\n\n[[hooks.pre-commit]]\nrun = \"x\"\n"
    ));
    let Err(ConfigError::Parse(e)) = got else {
        panic!("a misspelt key parsed: {got:?}");
    };
    assert!(e.to_string().contains("publc_branch"), "{e}");
}

#[test]
fn a_refusal_with_nothing_retired_keeps_its_line() {
    let got = Config::parse(&format!(
        "{BASE}\n\n\n[defaults]\npublc_branch = \"main\"\n"
    ));
    let Err(ConfigError::Parse(e)) = got else {
        panic!("a misspelt key parsed: {got:?}");
    };
    assert!(e.to_string().contains("line 7"), "{e}");
}

#[test]
fn what_is_said_names_the_file_where_the_hooks_moved_and_the_move() {
    let r = Retired {
        table: "hooks".into(),
        file:  "homma.local.toml".into(),
    };
    let said = r.said(Path::new("git/shook.toml"));
    for part in [
        "[hooks] is no longer read",
        "each row of it in homma.local.toml",
        "declared in git/shook.toml",
        "[hooks.<event>]",
        "`{paths}`",
        "take [hooks] out of homma.local.toml",
    ] {
        assert!(said.contains(part), "{part} in {said}");
    }
}
