//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma rules render` and `rules load` with the principles the caller names.

use std::fs;

use crate::cli::{OutputFormat, PrinciplesArgs, SessionArgs};

/// A workspace with one rule and one principle.
fn workspace() -> (tempfile::TempDir, homma_core::Config, PrinciplesArgs) {
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path().join(".shared/rules");
    fs::create_dir_all(&r).unwrap();
    fs::write(
        r.join("a-rule.md.tmpl"),
        "---\ntopics: [a]\nfires: \"always\"\nkind: reflex\n---\n\n# a-rule\n\nCard.\n",
    )
    .unwrap();
    let p = ws.path().join("principles");
    fs::create_dir_all(&p).unwrap();
    fs::write(
        p.join("reuse.md.tmpl"),
        "---\ntopics: [reuse]\nfires: \"always\"\nkind: reflex\n---\n\n# Principle: reuse\n\nUse what exists.\n",
    )
    .unwrap();
    let cfg = homma_core::Config::parse(&format!(
        "content_repo = \"c\"\n[workspace]\nname = \"w\"\npath = {:?}\n",
        ws.path().display().to_string()
    ))
    .unwrap();
    let args = PrinciplesArgs {
        principles:       Some(p),
        principle_prefix: Some("principle-".into()),
        principle_title:  Some("Principle: ".into()),
    };
    (ws, cfg, args)
}

fn session() -> SessionArgs {
    SessionArgs {
        role:  None,
        place: Some(homma_api::session::Place::Local),
    }
}

/// Failing control: with `load_cards` reading the rules only, no principle card is written.
#[test]
fn render_writes_a_principle_card_from_the_named_directory() {
    let (ws, cfg, args) = workspace();
    super::render::run(&cfg, &session(), &args, OutputFormat::Json).unwrap();
    let card = fs::read_to_string(ws.path().join(".claude/rules/principle-reuse.md")).unwrap();
    assert!(card.starts_with("# Principle: reuse"), "{card}");
    assert!(ws.path().join(".claude/rules/a-rule.md").is_file());
}

/// Failing control: with the principles left out of the row, it counts one card.
#[test]
fn load_counts_the_principle_card() {
    let (_ws, cfg, args) = workspace();
    let corpus = super::load_cards(&cfg, &args).unwrap();
    let session = homma_api::session::Session::every(homma_api::session::Place::Local);
    let row = super::load::row(&corpus, &session, true).unwrap();
    assert_eq!(row.cards, 2);
    assert!(row.names.iter().any(|c| c.name == "principle-reuse"));
}
