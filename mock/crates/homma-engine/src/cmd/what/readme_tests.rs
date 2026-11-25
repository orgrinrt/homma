//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The readme beside a tool's descriptor: its usage lines are verbs, its prose is the rest of the tool's text,
//! and a question about how to do something with a tool finds the line that says how.

use std::path::Path;

use super::doc::Kind;
use super::sources::tools;
use super::{Answer, answer};

const PIGEON_TOML: &str = "[tool]\nname=\"pigeon\"\nsummary=\"carries notes between lofts\"\ntags=[\"mail\"]\nbackend=\"local\"\npromote=true\n[tool.source]\npath={path=\"tools/pigeon\"}\n[[tool.commands]]\nname=\"pigeon\"\nsummary=\"pigeon <loft> [verb]: read, send, show\"\nrun=\"pigeon\"\n";

const PIGEON_README: &str = "# pigeon\n\nThe messenger. Prose that says pigeon loft show is not a usage line.\n\n```\npigeon loft                              what is new for me, marked read\npigeon loft show 3f9a1c                  one message whole, read or not\npigeon loft show --from wren [-n 2]  the newest from one name, whole\ncargo build                            not this tool's line\n```\n\nAfter the block, pigeon loft wait is prose again.\n";

fn plant(root: &Path, name: &str, toml: &str, readme: Option<&str>) {
    let d = root.join("tools").join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("tool.toml"), toml).unwrap();
    if let Some(r) = readme {
        std::fs::write(d.join("README.md"), r).unwrap();
    }
}

fn ask(root: &Path, q: &str) -> Answer {
    let g = tools::gather(root);
    answer(&g.docs, g.gaps, q, 5)
}

#[cfg(unix)]
#[test]
fn a_usage_line_in_a_fenced_block_is_a_verb_with_its_summary_and_its_dispatched_command() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, Some(PIGEON_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").expect("pigeon");
    let show = d
        .verbs
        .iter()
        .find(|v| v.name == "loft show 3f9a1c")
        .expect("the show line is a verb");
    assert_eq!(show.summary, "one message whole, read or not");
    assert_eq!(show.command, "homma pigeon loft show 3f9a1c");
    let from = d
        .verbs
        .iter()
        .find(|v| v.name == "loft show --from wren [-n 2]")
        .expect("a line with an option in it is a verb");
    assert_eq!(from.command, "homma pigeon loft show --from wren [-n 2]");
}

#[cfg(unix)]
#[test]
fn a_line_that_is_prose_or_another_programs_is_not_a_usage_line() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, Some(PIGEON_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert!(
        !d.verbs.iter().any(|v| v.name.contains("build")),
        "cargo build is not pigeon's"
    );
    assert!(
        !d.verbs.iter().any(|v| v.name.contains("wait")),
        "prose outside a fence is not a usage line"
    );
    assert!(
        !d.verbs.iter().any(|v| v.name.contains("is not a usage")),
        "prose naming the tool is not one"
    );
}

#[cfg(unix)]
#[test]
fn a_question_about_reading_an_earlier_message_finds_the_form_that_does_it() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, Some(PIGEON_README));
    let a = ask(ws.path(), "read an earlier message from a pigeon loft");
    let h = a.hits.first().expect("the question is answered");
    assert_eq!((h.kind, h.name.as_str()), (Kind::Tool, "pigeon"));
    assert_eq!(h.command, "homma pigeon loft show 3f9a1c");
}

#[cfg(unix)]
#[test]
fn without_a_readme_the_tool_is_read_by_its_descriptor_alone_and_nothing_is_said() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, None);
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert_eq!(d.verbs.len(), 1, "the one declared command");
    assert!(g.gaps.is_empty(), "a readme is not required");
}

#[cfg(unix)]
#[test]
fn a_command_the_descriptor_declares_is_not_read_twice_from_the_readme() {
    // The descriptor declares `show`, so `homma pigeon show` is its command; the readme's line `pigeon show  x`
    // dispatches to the same one. A line with a command of its own is the control.
    let toml = format!(
        "{PIGEON_TOML}[[tool.commands]]\nname=\"show\"\nsummary=\"read one\"\nrun=\"pigeon show\"\n"
    );
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "pigeon",
        &toml,
        Some("```\npigeon show  x\npigeon wait  y\n```\n"),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    let shown = d
        .verbs
        .iter()
        .filter(|v| v.command == "homma pigeon show")
        .count();
    assert_eq!(shown, 1, "{:?}", d.verbs);
    assert!(
        d.verbs
            .iter()
            .any(|v| v.command == "homma pigeon wait" && v.example),
        "a readme line the descriptor does not declare is still a verb"
    );
}

#[cfg(unix)]
#[test]
fn the_prose_of_the_readme_is_what_a_question_about_the_tool_can_match() {
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "pigeon",
        PIGEON_TOML,
        Some("# pigeon\n\nA retired bird is taken off the roster whole.\n"),
    );
    let a = ask(ws.path(), "retired bird");
    assert_eq!(a.hits.first().map(|h| h.name.as_str()), Some("pigeon"));
}

#[cfg(unix)]
#[test]
fn an_unclosed_fence_reads_to_the_end_and_a_fence_with_a_language_opens_one() {
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "pigeon",
        PIGEON_TOML,
        Some("```sh\npigeon loft show 1   one by id\n"),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert!(d.verbs.iter().any(|v| v.name == "loft show 1"));
}

#[cfg(unix)]
#[test]
fn a_readme_that_is_a_link_is_not_followed() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, None);
    let elsewhere = ws.path().join("elsewhere.md");
    std::fs::write(&elsewhere, PIGEON_README).unwrap();
    std::os::unix::fs::symlink(&elsewhere, ws.path().join("tools/pigeon/README.md")).unwrap();
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert_eq!(
        d.verbs.len(),
        1,
        "a link out of the tool's directory is not read"
    );
}

#[cfg(unix)]
#[test]
fn a_usage_line_is_marked_an_example_and_a_declared_command_is_not() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "pigeon", PIGEON_TOML, Some(PIGEON_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert!(
        d.verbs.iter().filter(|v| v.example).count() >= 2,
        "the readme's lines"
    );
    assert!(
        d.verbs.iter().any(|v| !v.example && v.name == "pigeon"),
        "the declared command"
    );
}

#[test]
fn the_words_of_a_usage_line_weigh_as_a_summary_and_not_as_a_verb_name() {
    use super::doc::{Doc, Verb};
    use super::rank::{Field, rank};
    let verb = |name: &str, example: bool| {
        Verb {
            name: name.into(),
            summary: String::new(),
            command: format!("x {name}"),
            example,
        }
    };
    let mut declared = Doc::new(Kind::Tool, "declared", "x", "other words");
    declared.verbs = vec![verb("zap", false)];
    let mut example = Doc::new(Kind::Tool, "example", "x", "other words");
    example.verbs = vec![verb("loft zap", true)];
    let docs = vec![example, declared];
    let hits = rank(&docs, &["zap".to_string()]);
    assert_eq!(
        docs[hits[0].doc].name, "declared",
        "a name outweighs a word of an example"
    );
    let by = |n: &str| hits.iter().find(|h| docs[h.doc].name == n).unwrap();
    assert_eq!(by("declared").matches[0].field, Field::VerbName);
    assert_eq!(by("example").matches[0].field, Field::VerbSummary);
}

#[cfg(unix)]
#[test]
fn the_prose_of_a_readme_stops_at_its_limit_inside_a_line() {
    let ws = tempfile::tempdir().unwrap();
    // One line of 100000 characters, none of them ASCII, so a cut by bytes would land short.
    let long = "ä".repeat(100_000);
    plant(
        ws.path(),
        "pigeon",
        PIGEON_TOML,
        Some(&format!("# pigeon\n\n{long}\n")),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    let kept = d.body.chars().count();
    assert!(
        (23_900 ..= 24_000).contains(&kept),
        "{kept} characters kept"
    );
    // The control: prose under the limit is kept whole.
    plant(
        ws.path(),
        "pigeon",
        PIGEON_TOML,
        Some("# pigeon\n\nshort prose\n"),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    assert_eq!(d.body, "# pigeon  short prose");
}

#[cfg(unix)]
#[test]
fn a_readme_keeps_at_most_eighty_usage_lines() {
    let ws = tempfile::tempdir().unwrap();
    let mut text = String::from("```\n");
    for i in 0 .. 200 {
        text.push_str(&format!("pigeon loft verb{i}   summary {i}\n"));
    }
    text.push_str("```\n");
    plant(ws.path(), "pigeon", PIGEON_TOML, Some(&text));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "pigeon").unwrap();
    let examples = d.verbs.iter().filter(|v| v.example).count();
    assert_eq!(examples, 80);
    // The first 80 are the ones kept.
    assert!(d.verbs.iter().any(|v| v.name == "loft verb79"));
    assert!(!d.verbs.iter().any(|v| v.name == "loft verb80"));
}
