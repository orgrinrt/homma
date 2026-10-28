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

const MSG_TOML: &str = "[tool]\nname=\"msg\"\nsummary=\"the messenger over channel branches\"\ntags=[\"mail\"]\nbackend=\"local\"\npromote=true\n[tool.source]\npath={path=\"tools/msg\"}\n[[tool.commands]]\nname=\"msg\"\nsummary=\"msg <channel> [verb]: read, post, show\"\nrun=\"msg\"\n";

const MSG_README: &str = "# msg\n\nThe messenger. Prose that says msg kaski show is not a usage line.\n\n```\nmsg kaski                              what is new for me, marked read\nmsg kaski show 3f9a1c                  one message whole, read or not\nmsg kaski show --from work-gui [-n 2]  the newest from one name, whole\ncargo build                            not this tool's line\n```\n\nAfter the block, msg kaski wait is prose again.\n";

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
    plant(ws.path(), "msg", MSG_TOML, Some(MSG_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").expect("msg");
    let show = d
        .verbs
        .iter()
        .find(|v| v.name == "kaski show 3f9a1c")
        .expect("the show line is a verb");
    assert_eq!(show.summary, "one message whole, read or not");
    assert_eq!(show.command, "homma msg kaski show 3f9a1c");
    let from = d
        .verbs
        .iter()
        .find(|v| v.name == "kaski show --from work-gui [-n 2]")
        .expect("a line with an option in it is a verb");
    assert_eq!(from.command, "homma msg kaski show --from work-gui [-n 2]");
}

#[cfg(unix)]
#[test]
fn a_line_that_is_prose_or_another_programs_is_not_a_usage_line() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "msg", MSG_TOML, Some(MSG_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
    assert!(
        !d.verbs.iter().any(|v| v.name.contains("build")),
        "cargo build is not msg's"
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
    plant(ws.path(), "msg", MSG_TOML, Some(MSG_README));
    let a = ask(ws.path(), "read an earlier message on a msg channel");
    let h = a.hits.first().expect("the question is answered");
    assert_eq!((h.kind, h.name.as_str()), (Kind::Tool, "msg"));
    assert_eq!(h.command, "homma msg kaski show 3f9a1c");
}

#[cfg(unix)]
#[test]
fn without_a_readme_the_tool_is_read_by_its_descriptor_alone_and_nothing_is_said() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "msg", MSG_TOML, None);
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
    assert_eq!(d.verbs.len(), 1, "the one declared command");
    assert!(g.gaps.is_empty(), "a readme is not required");
}

#[cfg(unix)]
#[test]
fn a_command_the_descriptor_declares_is_not_read_twice_from_the_readme() {
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "msg",
        MSG_TOML,
        Some("```\nmsg                 the bare command\n```\n"),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
    assert_eq!(
        d.verbs.len(),
        1,
        "a bare line adds nothing the declared command does not say"
    );
}

#[cfg(unix)]
#[test]
fn the_prose_of_the_readme_is_what_a_question_about_the_tool_can_match() {
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "msg",
        MSG_TOML,
        Some("# msg\n\nA retired member is taken off the branch whole.\n"),
    );
    let a = ask(ws.path(), "retired member");
    assert_eq!(a.hits.first().map(|h| h.name.as_str()), Some("msg"));
}

#[cfg(unix)]
#[test]
fn an_unclosed_fence_reads_to_the_end_and_a_fence_with_a_language_opens_one() {
    let ws = tempfile::tempdir().unwrap();
    plant(
        ws.path(),
        "msg",
        MSG_TOML,
        Some("```sh\nmsg kaski show 1   one by id\n"),
    );
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
    assert!(d.verbs.iter().any(|v| v.name == "kaski show 1"));
}

#[cfg(unix)]
#[test]
fn a_readme_that_is_a_link_is_not_followed() {
    let ws = tempfile::tempdir().unwrap();
    plant(ws.path(), "msg", MSG_TOML, None);
    let elsewhere = ws.path().join("elsewhere.md");
    std::fs::write(&elsewhere, MSG_README).unwrap();
    std::os::unix::fs::symlink(&elsewhere, ws.path().join("tools/msg/README.md")).unwrap();
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
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
    plant(ws.path(), "msg", MSG_TOML, Some(MSG_README));
    let g = tools::gather(ws.path());
    let d = g.docs.iter().find(|d| d.name == "msg").unwrap();
    assert!(
        d.verbs.iter().filter(|v| v.example).count() >= 2,
        "the readme's lines"
    );
    assert!(
        d.verbs.iter().any(|v| !v.example && v.name == "msg"),
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
    example.verbs = vec![verb("kaski zap", true)];
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
