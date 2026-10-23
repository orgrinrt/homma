//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A descriptor is read as renki's host reads it, and each way it can be wrong is refused with the
//! reason and the tool's name.

use std::path::Path;

use super::descriptor::*;

pub(super) const GOOD: &str = r#"
[tool]
name    = "msg"
summary = "the messenger"
tags    = ["coordination"]
backend = "local"
promote = true

[tool.source]
path = { path = "tools/msg" }

[[tool.commands]]
name    = "msg"
summary = "everything"
run     = "msg"

[[tool.commands]]
name    = "show"
summary = "one message"
run     = "commands/show"
"#;

fn parsed(text: &str) -> Result<Tool, String> {
    parse("msg", text, Path::new("/ws/tools/msg"))
}

fn with(from: &str, to: &str) -> String {
    assert!(GOOD.contains(from), "the fixture no longer holds {from}");
    GOOD.replacen(from, to, 1)
}

#[test]
fn a_descriptor_is_read_into_its_fields_and_verbs_in_order() {
    let t = parsed(GOOD).expect("accepted");
    assert_eq!(t.name, "msg");
    assert_eq!(t.summary, "the messenger");
    assert_eq!(t.tags, ["coordination"]);
    assert_eq!(t.backend, "local");
    assert!(t.promote);
    assert_eq!(t.dir, Path::new("/ws/tools/msg"));
    let verbs: Vec<(&str, &str)> = t
        .verbs
        .iter()
        .map(|v| (v.name.as_str(), v.run.as_str()))
        .collect();
    assert_eq!(verbs, [("msg", "msg"), ("show", "commands/show")]);
}

#[test]
fn promote_and_tags_and_commands_default_to_none() {
    let t = parsed("[tool]\nname=\"msg\"\nsummary=\"s\"\nbackend=\"local\"\n[tool.source]\npath={path=\"p\"}\n")
        .expect("accepted");
    assert!(!t.promote && t.tags.is_empty() && t.verbs.is_empty());
}

#[test]
fn a_git_source_is_a_source_too() {
    let text = with(
        "path = { path = \"tools/msg\" }",
        "git = { url = \"https://example.com/m.git\", rev = \"0123456789abcdef\" }",
    );
    assert!(parsed(&text).is_ok());
}

#[test]
fn a_name_that_is_not_the_directorys_is_refused_naming_both() {
    let e = parsed(&with(
        "name    = \"msg\"\nsummary = \"the",
        "name    = \"other\"\nsummary = \"the",
    ))
    .unwrap_err();
    assert!(e.contains("`other`") && e.contains("`msg`"), "{e}");
}

#[test]
fn a_field_the_format_has_no_name_for_is_refused() {
    let e = parsed(&with(
        "promote = true",
        "promote = true\nentrypoint = \"x\"",
    ))
    .unwrap_err();
    assert!(e.contains("entrypoint"), "{e}");
    let e = parsed(&with("run     = \"msg\"", "run     = \"msg\"\nflags = 1")).unwrap_err();
    assert!(e.contains("flags"), "{e}");
}

#[test]
fn a_missing_backend_source_or_summary_is_refused() {
    for (from, to) in [
        ("backend = \"local\"\n", ""),
        ("[tool.source]\npath = { path = \"tools/msg\" }\n", ""),
        ("summary = \"the messenger\"\n", ""),
    ] {
        assert!(
            parsed(&with(from, to)).is_err(),
            "accepted without {from:?}"
        );
    }
}

#[test]
fn an_empty_summary_or_backend_is_refused() {
    assert!(
        parsed(&with("summary = \"the messenger\"", "summary = \"  \""))
            .unwrap_err()
            .contains("summary")
    );
    assert!(
        parsed(&with("backend = \"local\"", "backend = \"\""))
            .unwrap_err()
            .contains("backend")
    );
}

#[test]
fn a_verb_given_twice_or_not_a_name_is_refused() {
    let e = parsed(&with("name    = \"show\"", "name    = \"msg\"")).unwrap_err();
    assert!(e.contains("given twice"), "{e}");
    let e = parsed(&with("name    = \"show\"", "name    = \"Show Me\"")).unwrap_err();
    assert!(e.contains("not a name"), "{e}");
}

#[test]
fn a_run_that_leaves_the_tool_or_is_empty_is_refused() {
    for run in ["/bin/sh", "../other/x", "commands/../../x", "", "./show"] {
        let e = parsed(&with(
            "run     = \"commands/show\"",
            &format!("run     = \"{run}\""),
        ))
        .unwrap_err();
        assert!(e.contains("not a path inside the tool"), "{run:?}: {e}");
    }
}

#[test]
fn text_that_is_not_toml_is_refused_with_a_reason() {
    assert!(
        parsed("this is not toml [")
            .unwrap_err()
            .contains("could not read it")
    );
}

fn tool_dir(root: &Path, name: &str, text: Option<&str>) {
    let d = root.join(TOOLS_DIR).join(name);
    std::fs::create_dir_all(&d).unwrap();
    if let Some(t) = text {
        std::fs::write(d.join(DESCRIPTOR), t).unwrap();
    }
}

#[test]
fn discovery_lists_each_descriptor_sorted_and_ignores_a_directory_without_one() {
    let ws = tempfile::tempdir().unwrap();
    tool_dir(ws.path(), "msg", Some(GOOD));
    tool_dir(ws.path(), "alpha", Some(&GOOD.replace("msg", "alpha")));
    tool_dir(ws.path(), "no-descriptor", None);
    let names: Vec<String> = discover(ws.path())
        .iter()
        .map(|e| e.name().to_string())
        .collect();
    assert_eq!(names, ["alpha", "msg"]);
}

#[test]
fn a_broken_descriptor_is_listed_by_its_directory_with_the_reason() {
    let ws = tempfile::tempdir().unwrap();
    tool_dir(ws.path(), "msg", Some(GOOD));
    tool_dir(ws.path(), "wake", Some(&GOOD.replace("msg", "other")));
    tool_dir(ws.path(), "junk", Some("[[[["));
    let found = discover(ws.path());
    assert_eq!(found.len(), 3);
    let broken: Vec<&Broken> = found
        .iter()
        .filter_map(|e| {
            match e {
                Entry::Broken(b) => Some(b),
                Entry::Tool(_) => None,
            }
        })
        .collect();
    assert_eq!(broken.len(), 2);
    assert_eq!(broken[0].name, "junk");
    assert_eq!(broken[1].name, "wake");
    assert!(broken[1].why.contains("`other`"), "{}", broken[1].why);
    assert!(broken[1].file.ends_with("tools/wake/tool.toml"));
}

#[test]
fn no_tools_directory_is_no_tools() {
    let ws = tempfile::tempdir().unwrap();
    assert!(discover(ws.path()).is_empty());
}
