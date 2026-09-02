//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests for what the shared-configs directory loads, and what it refuses.
//!
//! Separate from the comparison tests beside them because they answer a
//! different question: those ask what a repo owes given a set of templates,
//! and these ask which templates there are at all. Everything here refuses
//! rather than deciding, which is the point: a directory that cannot be read
//! unambiguously must say so instead of quietly loading fewer templates than
//! somebody put there.

use super::tests::Fixture;
use super::*;

#[test]
fn the_readme_beside_the_templates_is_not_one() {
    let f = Fixture::new(&[
        ("", "README.md", "# the configs\n"),
        ("rust_required", "README.md", "# the rust ones\n"),
        ("rust_required", "deny.toml", "x\n"),
    ]);
    let names: Vec<_> = f.templates().into_iter().map(|t| t.file_name).collect();
    assert_eq!(names, vec!["deny.toml".to_string()]);
}

#[test]
fn a_directory_naming_an_unknown_ecosystem_is_refused_at_load() {
    // Not skipped and not treated as untagged. A tag directory somebody spelled
    // wrong would otherwise turn a required config into a silently unplaced
    // one, which is the failure this whole directory exists to prevent.
    let f = Fixture::new(&[("ruby_required", "gemfile", "a\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::BadTag(..))));
}

#[test]
fn one_file_under_two_tag_directories_is_refused_at_load() {
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", "a\n"),
        ("deno_suggested", "deny.toml", "b\n"),
    ]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::Conflict(..))));
}

#[test]
fn a_tagged_file_conflicts_with_an_untagged_one_of_the_same_name() {
    let f = Fixture::new(&[("", "deny.toml", "a\n"), ("rust_required", "deny.toml", "b\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::Conflict(..))));
}

#[test]
fn one_directory_naming_an_ecosystem_twice_is_refused_at_load() {
    let f = Fixture::new(&[("rust_required+rust_suggested", "deny.toml", "a\n")]);
    assert!(matches!(templates(&f.ws), Err(TemplateError::BadTag(..))));
}

#[test]
fn a_missing_configs_directory_is_an_error_rather_than_an_empty_list() {
    // An empty list would make every repo pass, which is a check reporting
    // success because it could not run.
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        templates(dir.path()),
        Err(TemplateError::Missing(_))
    ));
}

#[test]
fn a_well_formed_directory_loads_rather_than_refusing() {
    // The control on every refusal above. Without it, each of them would pass
    // against a loader that refused everything, and the suite would read as
    // careful while the mechanism was broken.
    let f = Fixture::new(&[
        ("rust_required", "deny.toml", "a\n"),
        ("deno_suggested", "deno-lint.json", "b\n"),
        ("any", "editorconfig", "c\n"),
        ("", "mystery.toml", "?\n"),
    ]);
    let loaded = templates(&f.ws).expect("a well-formed directory must load");
    let names: Vec<_> = loaded.iter().map(|t| t.file_name.as_str()).collect();
    assert_eq!(names, vec![
        "deno-lint.json",
        "deny.toml",
        "editorconfig",
        "mystery.toml"
    ]);
    // And the untagged one is carried rather than dropped, because it is
    // reported per repo rather than silently skipped here.
    let mystery = loaded
        .iter()
        .find(|t| t.file_name == "mystery.toml")
        .unwrap();
    assert!(mystery.tags.is_empty());
}

#[test]
fn the_shared_rustfmt_config_is_the_reason_the_nightly_set_exists() {
    // The hand check, kept and re-run rather than written down as a number. The
    // shared copy is not merely a config we happen to use on nightly: most of
    // what it sets does not exist on stable at all, so a stable repo given it
    // formats to the defaults and warns once per option while doing so. That is
    // a property of the file, and the only thing that knows which options are
    // unstable is rustfmt itself.
    //
    // Skipped where a stable rustfmt is not installed. The set's behaviour is
    // covered by the cases beside this one regardless; this is about the actual
    // file, and a machine without stable cannot ask it.
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(CONFIGS_DIR);
    let Some(body) = ["rustfmt.toml", "rust_nightly_required/rustfmt.toml"]
        .iter()
        .find_map(|rel| std::fs::read(base.join(rel)).ok())
    else {
        return;
    };
    let probe = tempfile::tempdir().unwrap();
    std::fs::write(probe.path().join("rustfmt.toml"), &body).unwrap();
    std::fs::write(probe.path().join("x.rs"), "fn main() {}\n").unwrap();
    let Ok(out) = std::process::Command::new("rustfmt")
        .args(["+stable", "--check", "--edition", "2021", "x.rs"])
        .current_dir(probe.path())
        .output()
    else {
        return;
    };
    let complaints = String::from_utf8_lossy(&out.stderr);
    if complaints.contains("toolchain 'stable' is not installed")
        || complaints.contains("no such command")
    {
        return;
    }
    let unstable = complaints
        .lines()
        .filter(|l| l.contains("unstable features are only available in nightly"))
        .count();
    assert!(
        unstable >= 40,
        "only {unstable} of the shared rustfmt options are nightly-only, out of {}; if that is \
         real, a stable variant is now writable and the nightly set can go.\n{complaints}",
        String::from_utf8_lossy(&body)
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .count()
    );
}
