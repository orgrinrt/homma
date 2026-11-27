use std::path::Path;

use super::{Config, ConfigError, Retired};

const BASE: &str = "[workspace]\nname = \"w\"\n";

#[test]
fn a_manifest_still_carrying_the_git_hooks_table_parses_and_names_it() {
    let cfg = Config::parse(&format!(
        "{BASE}\n[[hooks.pre-commit]]\nrun = \"lintwhistle check {{paths}}\"\npaths = [\"*.md\"]\n"
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

#[test]
fn a_retired_table_comes_out_before_the_probe_and_an_unread_entry_beside_it_is_named_too() {
    let cfg = Config::parse(&format!(
        "{BASE}\n[[hooks.pre-commit]]\nrun = \"x\"\n\n[later]\nsomething = 1\n"
    ))
    .unwrap();
    assert_eq!(cfg.retired.len(), 1);
    assert_eq!(
        cfg.unread,
        vec!["later".to_string()],
        "the probe read what was left"
    );
    // the retired table is never read as an unread entry or a misspelling
    assert!(!cfg.unread.iter().any(|u| u.starts_with("hooks")));
}

#[test]
fn a_refusal_after_a_retired_table_keeps_its_line() {
    // line 7 is the misspelt key, with the retired table above it
    let got = Config::parse(&format!(
        "{BASE}\n[[hooks.pre-commit]]\nrun = \"x\"\n[defaults]\npublc_branch = \"main\"\n"
    ));
    let Err(ConfigError::Parse(e)) = got else {
        panic!("a misspelt key parsed: {got:?}");
    };
    assert!(e.to_string().contains("line 7"), "{e}");
}
