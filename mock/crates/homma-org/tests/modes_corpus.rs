//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Modes: one file each, generated into the skills tree as a skill whose
//! frontmatter is built from the mode's meta.

use std::fs;
use std::path::Path;

use homma_org::modes::{Modes, ModesError};

fn mode(dir: &Path, file: &str, kind: &str, body: &str) {
    fs::write(
        dir.join(format!("{file}.md.tmpl")),
        format!(
            "---\ntype: {kind}\neffort: high\nwhen: \"reviewing a diff\"\nskills: [a]\ntools: [b]\nends_turn: false\n---\n\n{body}"
        ),
    )
    .unwrap();
}

fn tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("modes")).unwrap();
    mode(
        &d.path().join("modes"),
        "review",
        "review",
        "- Read the design first.\n",
    );
    fs::write(d.path().join("modes/defaults.toml"), "[modes]\n").unwrap();
    d
}

/// Failing control: with the body written without the built frontmatter the
/// first assertion fails on the missing `effort: high` line.
#[test]
fn a_mode_is_written_as_a_skill_with_its_frontmatter_built_from_the_meta() {
    let d = tree();
    let m = Modes::load(&d.path().join("modes"), "mode-", 500).unwrap();
    assert_eq!(m.modes.len(), 1, "the settings file is not a mode");
    let out = d.path().join("skills");
    let written = m.render(&out).unwrap();
    assert_eq!(written, [out.join("mode-review/SKILL.md")]);
    assert_eq!(
        fs::read_to_string(&written[0]).unwrap(),
        "---\nname: mode-review\ndescription: \"mode: review. Use when reviewing a diff.\"\neffort: high\n---\n\n- Read the design first.\n"
    );
    assert!(m.claims("mode-review"));
}

#[test]
fn a_type_other_than_the_file_name_is_refused() {
    let d = tree();
    mode(&d.path().join("modes"), "review", "reveiw", "Body.\n");
    assert!(matches!(
        Modes::load(&d.path().join("modes"), "mode-", 500),
        Err(ModesError::TypeMismatch { declared, .. }) if declared == "reveiw"
    ));
}

#[test]
fn a_body_over_the_limit_is_refused() {
    let d = tree();
    assert!(
        Modes::load(&d.path().join("modes"), "mode-", 24).is_ok(),
        "24 characters fit"
    );
    assert!(matches!(
        Modes::load(&d.path().join("modes"), "mode-", 23),
        Err(ModesError::TooLong {
            chars: 24,
            max: 23,
            ..
        })
    ));
}

#[test]
fn remove_takes_out_an_earlier_mode_skill_and_nothing_else() {
    let d = tree();
    let m = Modes::load(&d.path().join("modes"), "mode-", 500).unwrap();
    let out = d.path().join("skills");
    m.render(&out).unwrap();
    fs::create_dir_all(out.join("other")).unwrap();
    assert_eq!(m.remove(&out).unwrap(), ["mode-review"]);
    assert!(!out.join("mode-review").exists());
    assert!(out.join("other").is_dir());
    assert!(m.remove(&out).unwrap().is_empty(), "nothing left to remove");
}
