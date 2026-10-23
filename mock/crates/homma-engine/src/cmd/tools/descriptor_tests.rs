//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A descriptor is read by renki and refused for what renki refuses, and the two checks that are homma's
//! own are made on top. Each way it can be wrong is refused with the reason and the tool's name.

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

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn parsed(text: &str) -> Result<renki::extension::Descriptor, String> {
    parse("msg", text)
}

fn with(from: &str, to: &str) -> String {
    assert!(GOOD.contains(from), "the fixture no longer holds {from}");
    GOOD.replacen(from, to, 1)
}

fn git_source(url: &str, rev: &str) -> String {
    with(
        "path = { path = \"tools/msg\" }",
        &format!("git = {{ url = \"{url}\", rev = \"{rev}\" }}"),
    )
}

#[test]
fn a_descriptor_is_read_into_its_fields_and_verbs_in_order() {
    let t = parsed(GOOD).expect("accepted");
    assert_eq!(t.name, "msg");
    assert_eq!(t.summary, "the messenger");
    assert_eq!(t.tags, ["coordination"]);
    assert_eq!(t.backend, "local");
    assert!(t.promote);
    let verbs: Vec<(&str, &str)> = t
        .commands
        .iter()
        .map(|v| (v.name.as_str(), v.run.as_str()))
        .collect();
    assert_eq!(verbs, [("msg", "msg"), ("show", "commands/show")]);
}

#[test]
fn promote_and_tags_and_commands_default_to_none() {
    let t = parsed("[tool]\nname=\"msg\"\nsummary=\"s\"\nbackend=\"local\"\n[tool.source]\npath={path=\"p\"}\n")
        .expect("accepted");
    assert!(!t.promote && t.tags.is_empty() && t.commands.is_empty());
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
fn a_verb_given_twice_is_refused_naming_it() {
    let e = parsed(&with("name    = \"show\"", "name    = \"msg\"")).unwrap_err();
    assert!(e.contains("`msg`") && e.contains("given twice"), "{e}");
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
fn a_run_that_leaves_the_tool_is_refused_and_one_that_stays_in_it_is_not() {
    for run in ["/bin/sh", "../other/x", "commands/../../x", "", "-x"] {
        let e = parsed(&with(
            "run     = \"commands/show\"",
            &format!("run     = \"{run}\""),
        ))
        .unwrap_err();
        assert!(e.contains("not inside it"), "{run:?}: {e}");
    }
    // renki reads a leading `./` as the tool's own directory, so it is a path inside it
    for run in ["./show", "./commands/show", "a..b"] {
        assert!(
            parsed(&with(
                "run     = \"commands/show\"",
                &format!("run     = \"{run}\"")
            ))
            .is_ok(),
            "{run:?} was refused"
        );
    }
}

#[test]
fn a_git_source_wants_a_full_object_name_and_a_url_git_cannot_read_as_a_flag() {
    assert!(parsed(&git_source("https://example.com/m.git", REV)).is_ok());
    assert!(parsed(&git_source("ssh://git@github.com/o/m.git", &"0".repeat(64))).is_ok());
    for (url, rev) in [
        ("https://example.com/m.git", "0123456789abcdef"),
        ("https://example.com/m.git", "abc"),
        ("https://example.com/m.git", "--upload-pack=x"),
        ("--upload-pack=x", REV),
        ("ssh://-oProxyCommand=x", REV),
        ("file:///tmp/m", REV),
    ] {
        let e = parsed(&git_source(url, rev)).unwrap_err();
        assert!(
            e.contains("`msg`") && e.contains("which is not one"),
            "{url} {rev}: {e}"
        );
    }
}

#[test]
fn a_path_source_that_climbs_or_is_empty_is_refused() {
    for p in ["../outside", "/abs", "", "-x"] {
        assert!(
            parsed(&with(
                "path = { path = \"tools/msg\" }",
                &format!("path = {{ path = \"{p}\" }}")
            ))
            .is_err(),
            "{p:?} was accepted"
        );
    }
}

#[test]
fn text_that_is_not_toml_is_refused_with_a_reason() {
    assert!(
        parsed("this is not toml [")
            .unwrap_err()
            .contains("could not read")
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
fn a_broken_descriptor_is_listed_by_its_directory_with_the_reason_on_one_line() {
    let ws = tempfile::tempdir().unwrap();
    tool_dir(ws.path(), "msg", Some(GOOD));
    tool_dir(ws.path(), "wake", Some(&GOOD.replace("msg", "other")));
    tool_dir(ws.path(), "junk", Some("[[[["));
    tool_dir(
        ws.path(),
        "short",
        Some(&git_source("https://e.com/m.git", "abc").replace("msg", "short")),
    );
    let found = discover(ws.path());
    assert_eq!(found.len(), 4);
    let broken: Vec<&Broken> = found
        .iter()
        .filter_map(|e| {
            match e {
                Entry::Broken(b) => Some(b),
                Entry::Tool(_) => None,
            }
        })
        .collect();
    let names: Vec<&str> = broken.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["junk", "short", "wake"]);
    assert!(broken.iter().all(|b| !b.why.contains('\n')));
    assert!(broken[2].why.contains("`other`"), "{}", broken[2].why);
    assert!(broken[1].why.contains("`abc`"), "{}", broken[1].why);
    assert!(broken[2].file.ends_with("tools/wake/tool.toml"));
}

#[test]
fn no_tools_directory_is_no_tools() {
    let ws = tempfile::tempdir().unwrap();
    assert!(discover(ws.path()).is_empty());
}
