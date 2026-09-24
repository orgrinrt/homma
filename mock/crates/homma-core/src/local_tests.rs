//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::Config;

const MANIFEST: &str = "[workspace]\nname = \"w\"\n";

fn ws() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), MANIFEST).unwrap();
    d
}

// ---------------------------------------------------------------------------
// the schema
// ---------------------------------------------------------------------------

#[test]
fn the_work_alone_is_a_whole_file() {
    let l = Local::parse("[instance]\nwork = \"kenno\"\n").unwrap();
    assert_eq!(l.instance.work, "kenno");
    assert_eq!(l.instance.title, None);
    assert!(l.instance.repos.is_empty());
    assert_eq!(l.instance.state, None);
    assert_eq!(l.instance.goal, None);
    assert!(l.tools.is_empty());
}

#[test]
fn every_instance_key_is_read() {
    let l = Local::parse(
        r#"
        [instance]
        work = "kenno"
        title = "the tile machine"
        repos = ["kenno", "homma"]
        state = ".shared/state/kenno.md"
        goal = ".data/op-responses/goal.md"
        "#,
    )
    .unwrap();
    assert_eq!(l.instance.title.as_deref(), Some("the tile machine"));
    assert_eq!(l.instance.repos, vec!["kenno", "homma"]);
    assert_eq!(
        l.instance.state,
        Some(PathBuf::from(".shared/state/kenno.md"))
    );
    assert_eq!(
        l.instance.goal,
        Some(PathBuf::from(".data/op-responses/goal.md"))
    );
}

#[test]
fn an_instance_without_its_work_is_refused() {
    assert!(Local::parse("[instance]\ntitle = \"t\"\n").is_err());
}

#[test]
fn a_file_without_an_instance_is_refused() {
    assert!(Local::parse("[tools.dashboard]\nurl = \"u\"\n").is_err());
    assert!(Local::parse("").is_err());
}

#[test]
fn an_unknown_instance_key_is_refused() {
    assert!(Local::parse("[instance]\nwork = \"k\"\nwrok = \"k\"\n").is_err());
}

#[test]
fn an_unknown_top_level_table_is_refused() {
    assert!(Local::parse("[instance]\nwork = \"k\"\n[workspace]\nname = \"w\"\n").is_err());
}

#[test]
fn repos_as_a_string_is_refused() {
    assert!(Local::parse("[instance]\nwork = \"k\"\nrepos = \"kenno\"\n").is_err());
}

#[test]
fn a_tool_table_is_carried_whatever_it_holds() {
    let l = Local::parse(
        r#"
        [instance]
        work = "k"
        [tools.dashboard]
        url = "https://example"
        max_age_minutes = 30
        benches = ["a", "b"]
        [tools.dashboard.nested]
        deep = true
        [tools.other]
        "#,
    )
    .unwrap();
    let d = &l.tools["dashboard"];
    assert_eq!(d["url"].as_str(), Some("https://example"));
    assert_eq!(d["max_age_minutes"].as_integer(), Some(30));
    assert_eq!(d["benches"].as_array().map(Vec::len), Some(2));
    assert_eq!(d["nested"]["deep"].as_bool(), Some(true));
    assert!(l.tools["other"].is_empty());
}

#[test]
fn a_tool_that_is_not_a_table_is_refused() {
    assert!(Local::parse("[instance]\nwork = \"k\"\n[tools]\ndashboard = 3\n").is_err());
}

// ---------------------------------------------------------------------------
// loading beside the manifest
// ---------------------------------------------------------------------------

#[test]
fn no_file_loads_as_none() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(Local::load(d.path()).unwrap(), None);
}

#[test]
fn a_file_loads_as_some() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\nwork = \"k\"\n").unwrap();
    assert_eq!(Local::load(d.path()).unwrap().unwrap().instance.work, "k");
}

#[test]
fn a_malformed_file_fails_naming_itself() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance\n").unwrap();
    let e = Local::load(d.path()).unwrap_err();
    assert!(matches!(e, LocalError::Parse { .. }), "{e:?}");
    assert!(e.to_string().contains(LOCAL_FILE), "{e}");
}

#[test]
fn an_unreadable_file_is_an_io_error_rather_than_absent() {
    // A directory where the file should be: present, and not readable as text.
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join(LOCAL_FILE)).unwrap();
    let e = Local::load(d.path()).unwrap_err();
    assert!(matches!(e, LocalError::Io { .. }), "{e:?}");
}

#[test]
fn a_malformed_file_does_not_stop_the_manifest_load() {
    // Every command loads the manifest, the gates included, and none of them
    // reads the instance. Each spelling of broken is tried: a missing key, bad
    // syntax, an unknown field, and a directory where the file should be.
    for broken in ["[instance]\n", "[instance\n", "[instance]\nwork = \"k\"\nx = 1\n"] {
        let d = ws();
        std::fs::write(d.path().join(LOCAL_FILE), broken).unwrap();
        assert!(
            Local::load(d.path()).is_err(),
            "the control: {broken:?} is broken"
        );
        Config::from_path(&d.path().join("homma.toml"))
            .unwrap_or_else(|e| panic!("{broken:?} stopped the manifest load: {e}"));
    }
    let d = ws();
    std::fs::create_dir(d.path().join(LOCAL_FILE)).unwrap();
    assert!(Local::load(d.path()).is_err());
    Config::from_path(&d.path().join("homma.toml")).expect("a directory there stops nothing");
}

#[test]
fn a_manifest_carrying_a_local_table_is_still_refused() {
    assert!(Config::parse("[workspace]\nname = \"w\"\n[local]\nwork = \"k\"\n").is_err());
}

// ---------------------------------------------------------------------------
// the skeleton
// ---------------------------------------------------------------------------

#[test]
fn the_skeleton_loads_and_carries_the_work() {
    let l = Local::parse(&skeleton("kenno")).unwrap();
    assert_eq!(l.instance.work, "kenno");
    assert!(l.instance.repos.is_empty());
}

#[test]
fn the_skeleton_names_every_instance_key() {
    let s = skeleton("k");
    for key in ["work", "title", "repos", "state", "goal"] {
        assert!(s.contains(&format!("{key} =")), "{key} missing from:\n{s}");
    }
}

#[test]
fn the_skeleton_quotes_a_work_name_toml_would_break_on() {
    let l = Local::parse(&skeleton("a \"quoted\" \\ name")).unwrap();
    assert_eq!(l.instance.work, "a \"quoted\" \\ name");
}

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

#[test]
fn set_writes_an_instance_key() {
    let out = set(&skeleton("k"), "instance.state", ".shared/state/k.md").unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.instance.state, Some(PathBuf::from(".shared/state/k.md")));
}

#[test]
fn set_replaces_an_existing_value() {
    let out = set(&skeleton("k"), "instance.work", "other").unwrap();
    assert_eq!(Local::parse(&out).unwrap().instance.work, "other");
}

#[test]
fn set_keeps_the_comments() {
    let text = "# top comment\n[instance]\nwork = \"k\" # trailing\n# below\n";
    let out = set(text, "instance.title", "T").unwrap();
    for kept in ["# top comment", "# trailing", "# below"] {
        assert!(out.contains(kept), "{kept} lost:\n{out}");
    }
}

#[test]
fn set_makes_a_tool_table_without_a_bare_tools_header() {
    let out = set(&skeleton("k"), "tools.dashboard.url", "https://x").unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.tools["dashboard"]["url"].as_str(), Some("https://x"));
    assert!(out.contains("[tools.dashboard]"), "{out}");
    assert!(!out.lines().any(|line| line.trim() == "[tools]"), "{out}");
}

#[test]
fn set_twice_on_a_tool_keeps_both_keys() {
    let one = set(&skeleton("k"), "tools.dashboard.url", "u").unwrap();
    let two = set(&one, "tools.dashboard.published", "abc").unwrap();
    let l = Local::parse(&two).unwrap();
    assert_eq!(l.tools["dashboard"]["url"].as_str(), Some("u"));
    assert_eq!(l.tools["dashboard"]["published"].as_str(), Some("abc"));
}

#[test]
fn set_leaves_another_tool_alone() {
    let one = set(&skeleton("k"), "tools.a.x", "1").unwrap();
    let two = set(&one, "tools.b.y", "2").unwrap();
    let l = Local::parse(&two).unwrap();
    assert_eq!(l.tools["a"]["x"].as_str(), Some("1"));
    assert_eq!(l.tools["b"]["y"].as_str(), Some("2"));
}

#[test]
fn set_refuses_every_key_outside_the_two_shapes() {
    for key in [
        "instance",
        "instance.",
        ".work",
        "tools",
        "tools.dashboard",
        "tools.dashboard.a.b",
        "workspace.name",
        "instance.work.deeper",
        "",
    ] {
        let e = set(&skeleton("k"), key, "v").unwrap_err();
        assert!(matches!(e, LocalError::Key(_)), "{key}: {e:?}");
    }
}

#[test]
fn set_refuses_a_value_that_leaves_the_file_unloadable() {
    // A string where the schema wants a list, and a key the schema has not got.
    for key in ["instance.repos", "instance.wrok"] {
        let e = set(&skeleton("k"), key, "v").unwrap_err();
        assert!(matches!(e, LocalError::Refused(_)), "{key}: {e:?}");
    }
}

#[test]
fn set_refuses_text_that_is_not_toml() {
    let e = set("[instance", "instance.work", "v").unwrap_err();
    assert!(matches!(e, LocalError::Edit(_)), "{e:?}");
}

#[test]
fn set_quotes_what_it_writes() {
    let out = set(&skeleton("k"), "instance.title", "a \"b\" = [c]").unwrap();
    assert_eq!(
        Local::parse(&out).unwrap().instance.title.as_deref(),
        Some("a \"b\" = [c]")
    );
}

// ---------------------------------------------------------------------------
// the ignore line
// ---------------------------------------------------------------------------

#[test]
fn a_missing_gitignore_is_made_with_the_line() {
    let d = tempfile::tempdir().unwrap();
    assert!(ensure_ignored(d.path()).unwrap());
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        "/homma.local.toml\n"
    );
}

#[test]
fn the_line_is_appended_after_a_missing_final_newline() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join(".gitignore"), "/target").unwrap();
    assert!(ensure_ignored(d.path()).unwrap());
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        "/target\n/homma.local.toml\n"
    );
}

#[test]
fn a_gitignore_already_naming_it_is_left_alone() {
    for existing in ["/homma.local.toml\n", "homma.local.toml\n", "a\n  /homma.local.toml  \nb\n"] {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".gitignore"), existing).unwrap();
        assert!(!ensure_ignored(d.path()).unwrap(), "{existing:?}");
        assert_eq!(
            std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
            existing
        );
    }
}

#[test]
fn a_line_that_only_resembles_it_does_not_count() {
    for near in ["/homma.local.toml.bak\n", "# /homma.local.toml\n", "/sub/homma.local.toml\n"] {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".gitignore"), near).unwrap();
        assert!(ensure_ignored(d.path()).unwrap(), "{near:?}");
    }
}

#[test]
fn ensuring_twice_writes_once() {
    let d = tempfile::tempdir().unwrap();
    assert!(ensure_ignored(d.path()).unwrap());
    assert!(!ensure_ignored(d.path()).unwrap());
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        "/homma.local.toml\n"
    );
}
