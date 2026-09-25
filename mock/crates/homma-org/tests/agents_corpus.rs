//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Loading a persona corpus and generating the personas a session dispatches.
//!
//! A persona is one file rendered whole, so what is worth testing is that the
//! file arrives as written, that the name it declares is held to its filename,
//! and what the pass refuses, reports and leaves alone.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_org::agents::{AgentsError, Personas};

/// A persona as the corpus holds them today: host fields this does not read,
/// one of them nested, and a body.
const REVIEWER: &str = "---\nname: reviewer\ndescription: Reviews a batch of pull requests.\ntools: Read, Grep\nmodel: opus\nexperimental:\n  cacheTtl: 1h\n---\n\n# You are a reviewer\n\nRead the diff.\n";

/// A directory unique per call, since the tests in one binary share a process id.
fn dir() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-agents-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("agents")).unwrap();
    dir
}

fn author(dir: &Path, file: &str, body: &str) {
    fs::write(dir.join("agents").join(file), body).unwrap();
}

#[test]
fn a_persona_arrives_byte_for_byte_nested_fields_included() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    let corpus = Personas::load(&d.join("agents")).unwrap();
    let written = corpus.render(&d.join("out")).unwrap();
    assert_eq!(written, vec![d.join("out/reviewer.md")]);
    assert_eq!(
        fs::read_to_string(d.join("out/reviewer.md")).unwrap(),
        REVIEWER
    );
}

#[test]
fn the_name_and_description_are_read_off_the_top_level() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    let corpus = Personas::load(&d.join("agents")).unwrap();
    assert_eq!(corpus.personas.len(), 1);
    assert_eq!(corpus.personas[0].name, "reviewer");
    assert_eq!(
        corpus.personas[0].description,
        "Reviews a batch of pull requests."
    );
}

#[test]
fn a_template_renders_against_its_own_name() {
    let d = dir();
    author(
        &d,
        "echo.md.tmpl",
        "---\nname: echo\ndescription: Says its name.\n---\n\nI am {{ name }}.\n",
    );
    let corpus = Personas::load(&d.join("agents")).unwrap();
    corpus.render(&d.join("out")).unwrap();
    assert_eq!(
        fs::read_to_string(d.join("out/echo.md")).unwrap(),
        "---\nname: echo\ndescription: Says its name.\n---\n\nI am echo.\n"
    );
}

#[test]
fn a_declared_name_that_is_not_the_filename_is_refused() {
    let d = dir();
    author(&d, "critic.md.tmpl", REVIEWER);
    match Personas::load(&d.join("agents")) {
        Err(AgentsError::NameMismatch {
            declared,
            file,
            ..
        }) => {
            assert_eq!((declared.as_str(), file.as_str()), ("reviewer", "critic"));
        },
        other => panic!("expected a name mismatch, got {other:?}"),
    }
}

#[test]
fn a_nested_name_is_not_read_as_the_declared_one() {
    // A host field nesting a `name:` under it must not stand in for the
    // persona's own, which here is missing.
    let d = dir();
    author(
        &d,
        "nested.md.tmpl",
        "---\ndescription: Nests a name.\nhooks:\n  name: nested\n---\n\nbody\n",
    );
    match Personas::load(&d.join("agents")) {
        Err(AgentsError::Missing {
            key,
            ..
        }) => assert_eq!(key, "name"),
        other => panic!("expected the name missing, got {other:?}"),
    }
}

#[test]
fn a_persona_with_no_description_is_refused() {
    let d = dir();
    author(
        &d,
        "quiet.md.tmpl",
        "---\nname: quiet\ndescription:\n---\n\nbody\n",
    );
    match Personas::load(&d.join("agents")) {
        Err(AgentsError::Missing {
            key,
            ..
        }) => assert_eq!(key, "description"),
        other => panic!("expected the description missing, got {other:?}"),
    }
}

#[test]
fn a_file_with_no_frontmatter_or_an_unclosed_one_is_refused() {
    for body in ["# no block\n", "---\nname: open\ndescription: never closes\n"] {
        let d = dir();
        author(&d, "open.md.tmpl", body);
        assert!(
            matches!(
                Personas::load(&d.join("agents")),
                Err(AgentsError::NoFrontmatter { .. })
            ),
            "{body:?}"
        );
    }
}

#[test]
fn a_quoted_name_is_read_without_its_quotes() {
    let d = dir();
    author(
        &d,
        "quoted.md.tmpl",
        "---\nname: \"quoted\"\ndescription: 'Quoted too.'\n---\n\nbody\n",
    );
    let corpus = Personas::load(&d.join("agents")).unwrap();
    assert_eq!(corpus.personas[0].name, "quoted");
    assert_eq!(corpus.personas[0].description, "Quoted too.");
}

#[test]
fn a_template_that_does_not_render_is_refused_and_nothing_is_written() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    author(
        &d,
        "zz-broken.md.tmpl",
        "---\nname: zz-broken\ndescription: Names a variable nobody defines.\n---\n\n{{ nobody }}\n",
    );
    let corpus = Personas::load(&d.join("agents")).unwrap();
    // The good one renders first, so a pass writing each as it went would have
    // written it before reaching the broken one; that is what the last
    // assertion can catch.
    let order: Vec<_> = corpus.personas.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(order, vec!["reviewer", "zz-broken"]);
    match corpus.render(&d.join("out")) {
        Err(AgentsError::Render {
            path,
            ..
        }) => assert!(path.ends_with("zz-broken.md.tmpl"), "{path:?}"),
        other => panic!("expected a render refusal, got {other:?}"),
    }
    assert!(!d.join("out/reviewer.md").exists());
    assert!(!d.join("out").exists(), "not even the directory is made");
}

#[test]
fn a_file_without_the_template_suffix_is_not_a_persona() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    author(
        &d,
        "README.md",
        "---\nname: kebab-case-id\n---\n\nthe corpus's own readme\n",
    );
    fs::create_dir_all(d.join("agents/drafts")).unwrap();
    let corpus = Personas::load(&d.join("agents")).unwrap();
    let names: Vec<_> = corpus.personas.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["reviewer"]);
    corpus.render(&d.join("out")).unwrap();
    assert!(!d.join("out/README.md").exists());
}

#[test]
fn a_generated_persona_whose_template_is_gone_is_named_and_left_alone() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    fs::create_dir_all(d.join("out")).unwrap();
    fs::write(d.join("out/retired.md"), "an old persona\n").unwrap();
    fs::write(d.join("out/notes.txt"), "not a persona\n").unwrap();
    let corpus = Personas::load(&d.join("agents")).unwrap();
    corpus.render(&d.join("out")).unwrap();
    assert_eq!(corpus.unclaimed(&d.join("out")).unwrap(), vec![
        "retired".to_string()
    ]);
    assert_eq!(
        fs::read_to_string(d.join("out/retired.md")).unwrap(),
        "an old persona\n"
    );
}

#[test]
fn a_hand_edit_to_a_generated_persona_is_rewritten() {
    let d = dir();
    author(&d, "reviewer.md.tmpl", REVIEWER);
    let corpus = Personas::load(&d.join("agents")).unwrap();
    corpus.render(&d.join("out")).unwrap();
    fs::write(d.join("out/reviewer.md"), "edited by hand\n").unwrap();
    corpus.render(&d.join("out")).unwrap();
    assert_eq!(
        fs::read_to_string(d.join("out/reviewer.md")).unwrap(),
        REVIEWER
    );
}

#[test]
fn an_absent_corpus_is_unreadable_and_an_empty_one_renders_nothing() {
    let d = dir();
    assert!(matches!(
        Personas::load(&d.join("nowhere")),
        Err(AgentsError::Unreadable { .. })
    ));
    let corpus = Personas::load(&d.join("agents")).unwrap();
    assert!(corpus.render(&d.join("out")).unwrap().is_empty());
    assert!(corpus.unclaimed(&d.join("absent")).unwrap().is_empty());
}

/// A persona whose `key` line is `line` and whose other key is plain.
fn with_line(key: &str, line: &str) -> String {
    match key {
        "name" => format!("---\n{line}\ndescription: Plain.\n---\n\nbody\n"),
        _ => format!("---\nname: odd\n{line}\n---\n\nbody\n"),
    }
}

#[test]
fn a_folded_or_literal_block_on_either_key_is_refused() {
    for key in ["name", "description"] {
        for indicator in [">", "|", ">-", "|+", ">2"] {
            let d = dir();
            // The block's text on the indented line below it, inside the
            // frontmatter where a folded value puts it.
            let line = format!("{key}: {indicator}");
            author(
                &d,
                "odd.md.tmpl",
                &with_line(key, &format!("{line}\n  odd")),
            );
            match Personas::load(&d.join("agents")) {
                Err(AgentsError::BadValue {
                    key: k,
                    value,
                    ..
                }) => {
                    assert_eq!(k, key);
                    assert_eq!(value, indicator);
                },
                other => panic!("`{line}` should be refused, got {other:?}"),
            }
        }
    }
}

#[test]
fn a_comment_after_either_key_is_refused() {
    for key in ["name", "description"] {
        let value = if key == "name" { "odd" } else { "Plain." };
        for line in [
            format!("{key}: {value} # a note"),
            format!("{key}: \"{value}\" # a note"),
            format!("{key}: '{value}' # a note"),
            format!("{key}: # nothing but a note"),
            format!("{key}: {value}\t# a note after a tab"),
            format!("{key}: \"{value}\" # and \""),
            format!("{key}: '{value}' # and '"),
        ] {
            let d = dir();
            author(&d, "odd.md.tmpl", &with_line(key, &line));
            assert!(
                matches!(
                    Personas::load(&d.join("agents")),
                    Err(AgentsError::BadValue { .. })
                ),
                "`{line}` should be refused"
            );
        }
    }
}

#[test]
fn a_hash_that_is_not_a_comment_is_read_as_text() {
    // The controls for the two arms above: a `#` with no space before it, and
    // a ` #` inside a value quoted whole, are both text.
    for (line, read) in [
        ("description: Reads C#, and F#.", "Reads C#, and F#."),
        (
            "description: \"Counts the # of rows.\"",
            "Counts the # of rows.",
        ),
        (
            "description: 'Counts the # of rows.'",
            "Counts the # of rows.",
        ),
        (
            "description: Pipes a|b and compares a>b.",
            "Pipes a|b and compares a>b.",
        ),
        // A quote escaped inside a value quoted whole does not end it, so the
        // ` #` after it is still text.
        (
            "description: \"Says \\\"hi\\\" # twice.\"",
            "Says \\\"hi\\\" # twice.",
        ),
        ("description: 'It''s # one.'", "It''s # one."),
    ] {
        let d = dir();
        author(&d, "odd.md.tmpl", &with_line("description", line));
        let corpus = Personas::load(&d.join("agents")).unwrap_or_else(|e| panic!("{line}: {e}"));
        assert_eq!(corpus.personas[0].description, read);
    }
}

#[test]
fn a_nested_line_shaped_like_a_bad_value_is_not_read() {
    // Only the top level is read, so a host field's nested `description: >`
    // is the host's business and refuses nothing.
    let d = dir();
    author(
        &d,
        "reviewer.md.tmpl",
        "---\nname: reviewer\ndescription: Reviews.\nhooks:\n  description: >\n    folded\n  name: x # note\n---\n\nbody\n",
    );
    let corpus = Personas::load(&d.join("agents")).unwrap();
    assert_eq!(corpus.personas[0].description, "Reviews.");
}

#[test]
fn a_generated_persona_ends_in_exactly_one_newline() {
    for tail in ["", "\n", "\n\n\n", "  \n\t\n"] {
        let d = dir();
        author(
            &d,
            "reviewer.md.tmpl",
            &format!("---\nname: reviewer\ndescription: Reviews.\n---\n\nbody{tail}"),
        );
        let corpus = Personas::load(&d.join("agents")).unwrap();
        corpus.render(&d.join("out")).unwrap();
        let out = fs::read_to_string(d.join("out/reviewer.md")).unwrap();
        assert!(out.ends_with("body\n"), "{tail:?} gave {out:?}");
    }
}
