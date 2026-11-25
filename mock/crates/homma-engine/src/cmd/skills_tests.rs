//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma skills render` with the modes the caller names and its effort flag.

use std::fs;
use std::path::Path;

use super::render;
use crate::cli::{ModesArgs, OutputFormat};

/// A workspace with one authored skill carrying `effort`, and one mode.
fn workspace(skill: &str) -> (tempfile::TempDir, homma_core::Config) {
    let ws = tempfile::tempdir().unwrap();
    let s = ws.path().join(".shared/skills").join(skill);
    fs::create_dir_all(&s).unwrap();
    fs::write(
        s.join("SKILL.md.tmpl"),
        format!("---\nname: {skill}\ndescription: Use when it is hard.\neffort: high\n---\n\n# {skill}\n"),
    )
    .unwrap();
    let m = ws.path().join("modes");
    fs::create_dir_all(&m).unwrap();
    fs::write(
        m.join("review.md.tmpl"),
        "---\ntype: review\neffort: high\nwhen: \"reviewing\"\nskills: []\ntools: []\nends_turn: false\n---\n\nRead first.\n",
    )
    .unwrap();
    let cfg = homma_core::Config::parse(&format!(
        "content_repo = \"c\"\n[workspace]\nname = \"w\"\npath = {:?}\n",
        ws.path().display().to_string()
    ))
    .unwrap();
    (ws, cfg)
}

fn modes(ws: &Path) -> ModesArgs {
    ModesArgs {
        modes:         Some(ws.join("modes")),
        mode_prefix:   Some("mode-".into()),
        mode_body_max: Some(500),
    }
}

fn skill_md(ws: &Path, name: &str) -> String {
    fs::read_to_string(ws.join(".claude/skills").join(name).join("SKILL.md")).unwrap()
}

/// Failing control: with the clash check taken out of `run`, both render and the
/// mode overwrites the authored skill.
#[test]
fn a_mode_whose_skill_name_an_authored_skill_has_is_refused() {
    let (ws, cfg) = workspace("mode-review");
    let err = render::run(&cfg, &modes(ws.path()), true, OutputFormat::Json).unwrap_err();
    assert!(err.to_string().contains("`mode-review`"), "{err}");
    assert!(
        !ws.path().join(".claude/skills/mode-review").exists(),
        "nothing was written"
    );
}

#[test]
fn without_the_flag_effort_is_kept_and_the_mode_skill_written() {
    let (ws, cfg) = workspace("deep");
    render::run(&cfg, &modes(ws.path()), true, OutputFormat::Json).unwrap();
    assert!(skill_md(ws.path(), "deep").contains("\neffort: high\n"));
    assert!(skill_md(ws.path(), "mode-review").contains("\neffort: high\n"));
}

/// Failing control: with `remove` taken out of `run`, the earlier mode skill stays.
#[test]
fn no_effort_takes_the_keys_out_and_removes_an_earlier_mode_skill() {
    let (ws, cfg) = workspace("deep");
    render::run(&cfg, &modes(ws.path()), true, OutputFormat::Json).unwrap();
    render::run(&cfg, &modes(ws.path()), false, OutputFormat::Json).unwrap();
    assert!(!ws.path().join(".claude/skills/mode-review").exists());
    let deep = skill_md(ws.path(), "deep");
    assert!(!deep.contains("effort:"), "{deep}");
    assert!(deep.contains("name: deep"), "the authored skill stays");
}
