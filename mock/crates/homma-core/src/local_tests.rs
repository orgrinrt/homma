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

const BOTH: &str = "/homma.local.toml\n/homma.local.toml.*\n";

fn ignore_after(before: Option<&str>) -> (bool, String) {
    let d = tempfile::tempdir().unwrap();
    if let Some(before) = before {
        std::fs::write(d.path().join(".gitignore"), before).unwrap();
    }
    let wrote = ensure_ignored(d.path()).unwrap();
    (
        wrote,
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
    )
}

#[test]
fn a_missing_gitignore_is_made_with_both_lines() {
    assert_eq!(ignore_after(None), (true, BOTH.to_owned()));
}

#[test]
fn the_lines_are_appended_after_a_missing_final_newline() {
    assert_eq!(
        ignore_after(Some("/target")),
        (true, format!("/target\n{BOTH}"))
    );
}

#[test]
fn a_gitignore_already_naming_both_is_left_alone() {
    for existing in [
        BOTH,
        "homma.local.toml\nhomma.local.toml.*\n",
        "a\n  /homma.local.toml.*  \nb\n  /homma.local.toml  \n",
    ] {
        assert_eq!(
            ignore_after(Some(existing)),
            (false, existing.to_owned()),
            "{existing:?}"
        );
    }
}

#[test]
fn a_gitignore_naming_only_the_file_gains_the_leftovers() {
    // Every clone set up before the second line existed is in this state.
    for existing in ["/homma.local.toml\n", "homma.local.toml\n"] {
        assert_eq!(
            ignore_after(Some(existing)),
            (true, format!("{existing}/homma.local.toml.*\n")),
            "{existing:?}"
        );
    }
}

#[test]
fn a_gitignore_naming_only_the_leftovers_gains_the_file() {
    assert_eq!(
        ignore_after(Some("/homma.local.toml.*\n")),
        (true, "/homma.local.toml.*\n/homma.local.toml\n".to_owned())
    );
}

#[test]
fn a_line_that_only_resembles_either_does_not_count() {
    for near in [
        "/homma.local.toml.bak\n",
        "# /homma.local.toml\n",
        "/sub/homma.local.toml\n",
        "# /homma.local.toml.*\n",
        "/homma.local.*\n",
        "/sub/homma.local.toml.*\n",
    ] {
        assert_eq!(
            ignore_after(Some(near)),
            (true, format!("{near}{BOTH}")),
            "{near:?}"
        );
    }
}

#[test]
fn ensuring_twice_writes_once() {
    let d = tempfile::tempdir().unwrap();
    assert!(ensure_ignored(d.path()).unwrap());
    assert!(!ensure_ignored(d.path()).unwrap());
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        BOTH
    );
}

#[test]
fn the_leftovers_line_ignores_exactly_what_set_leaves_behind() {
    // Asked of git itself, so the claim is about the pattern's meaning rather
    // than its spelling.
    let d = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(d.path())
            .output()
            .unwrap()
    };
    assert!(git(&["init", "-q"]).status.success());
    ensure_ignored(d.path()).unwrap();
    for (path, ignored) in [
        ("homma.local.toml", true),
        ("homma.local.toml.lock", true),
        ("homma.local.toml.4242.tmp", true),
        ("homma.toml", false),
        ("homma.local.tomlx", false),
        ("sub/homma.local.toml.lock", false),
    ] {
        let out = git(&["check-ignore", "-q", "--no-index", path]);
        assert_eq!(out.status.success(), ignored, "{path}");
    }
}

// ---------------------------------------------------------------------------
// filling an installed file from the stored config
// ---------------------------------------------------------------------------

const STORED: &str = "# the stored one\n[instance]\nwork = \"k\"\ntitle = \"stored title\"\nrepos = [\"a\"]\n\n[tools.goal]\nactive = \"false\"\nobjective = \"o\"\n\n[tools.msg]\nas = \"coord\"\n";

#[test]
fn fill_adds_every_key_the_file_lacks_and_names_each() {
    let (out, added) = fill("[instance]\nwork = \"k\"\n", STORED).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.instance.title.as_deref(), Some("stored title"));
    assert_eq!(l.instance.repos, vec!["a".to_owned()]);
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(l.tools["msg"]["as"].as_str(), Some("coord"));
    assert_eq!(added, [
        "instance.title",
        "instance.repos",
        "tools.goal.active",
        "tools.goal.objective",
        "tools.msg.as"
    ]);
}

#[test]
fn fill_never_changes_a_key_the_file_has() {
    let have = "[instance]\nwork = \"k\"\ntitle = \"mine\"\nrepos = []\n\n[tools.goal]\nactive = \"true\"\n";
    let (out, added) = fill(have, STORED).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.instance.title.as_deref(), Some("mine"));
    assert!(
        l.instance.repos.is_empty(),
        "a list is a value, not merged into"
    );
    assert_eq!(l.tools["goal"]["active"].as_str(), Some("true"));
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(added, ["tools.goal.objective", "tools.msg.as"]);
}

#[test]
fn fill_keeps_what_a_tool_recorded_and_the_store_never_held() {
    let have = "[instance]\nwork = \"k\"\n\n[tools.dashboard]\npublished = \"abc\"\nurl = \"u\"\n";
    let (out, _) = fill(have, STORED).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.tools["dashboard"]["published"].as_str(), Some("abc"));
    assert_eq!(l.tools["dashboard"]["url"].as_str(), Some("u"));
}

#[test]
fn fill_keeps_the_files_own_comments() {
    let have = "# mine\n[instance]\nwork = \"k\" # why\n";
    let (out, _) = fill(have, STORED).unwrap();
    assert!(out.starts_with("# mine\n"), "{out}");
    assert!(out.contains("work = \"k\" # why"), "{out}");
}

#[test]
fn fill_of_a_complete_file_adds_nothing_and_changes_no_byte() {
    let (once, _) = fill("[instance]\nwork = \"k\"\n", STORED).unwrap();
    let (twice, added) = fill(&once, STORED).unwrap();
    assert!(added.is_empty(), "{added:?}");
    assert_eq!(once, twice);
}

#[test]
fn fill_enters_an_inline_table_in_the_file_and_adds_the_key_it_lacks() {
    let have = "[instance]\nwork = \"k\"\n\n[tools]\ngoal = { active = \"true\" }\n";
    let (out, added) = fill(have, STORED).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.tools["goal"]["active"].as_str(), Some("true"));
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(added, [
        "instance.title",
        "instance.repos",
        "tools.goal.objective",
        "tools.msg.as"
    ]);
}

#[test]
fn fill_names_each_key_of_an_inline_stored_table_that_the_file_lacks() {
    let stored =
        "[instance]\nwork = \"k\"\n\n[tools]\ngoal = { active = \"false\", objective = \"o\" }\n";
    let (out, added) = fill("[instance]\nwork = \"k\"\n", stored).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(added, ["tools.goal.active", "tools.goal.objective"]);
    let (again, added) = fill(&out, stored).unwrap();
    assert!(added.is_empty(), "{added:?}");
    assert_eq!(out, again);
}

#[test]
fn fill_enters_an_inline_stored_table_over_a_file_that_writes_it_as_a_table() {
    let stored =
        "[instance]\nwork = \"k\"\n\n[tools]\ngoal = { active = \"false\", objective = \"o\" }\n";
    let have = "[instance]\nwork = \"k\"\n\n[tools.goal]\nactive = \"true\"\n";
    let (out, added) = fill(have, stored).unwrap();
    let l = Local::parse(&out).unwrap();
    assert_eq!(l.tools["goal"]["active"].as_str(), Some("true"));
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(added, ["tools.goal.objective"]);
}

#[test]
fn fill_refuses_text_that_is_not_toml() {
    assert!(matches!(fill("[[", STORED), Err(LocalError::Edit(_))));
    assert!(matches!(
        fill("[instance]\nwork = \"k\"\n", "[["),
        Err(LocalError::Edit(_))
    ));
}

#[test]
fn a_stored_config_is_the_one_its_name_says() {
    let p = Path::new("/store/k.toml");
    assert_eq!(parse_stored(p, "k", STORED).unwrap().instance.work, "k");
    match parse_stored(p, "other", STORED) {
        Err(LocalError::StoredWork {
            name,
            work,
        }) => assert_eq!((name.as_str(), work.as_str()), ("other", "k")),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        parse_stored(p, "k", "[instance]\n"),
        Err(LocalError::Parse { .. })
    ));
}

#[test]
fn the_clones_own_hooks_layer_is_accepted_beside_its_own_tables() {
    let l = Local::parse(
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"y\"\n\
         [[agent.hooks.Stop]]\nrun = \"scripts/z\"\n",
    )
    .unwrap();
    assert!(l.hooks.is_some());
    assert!(l.agent.is_some());
    // and a table nobody reads is still refused, which is the control
    assert!(
        Local::parse("[instance]\nwork = \"x\"\n[forges.github]\nkind = \"github\"\n").is_err()
    );
}

const WITH_CONSOLE: &str = "[instance]\nwork = \"k\"\n\n[tools.console]\nchannel = \"k\"\n";

#[test]
fn a_config_names_a_console_by_its_channel_and_nothing_else() {
    assert!(Local::parse(WITH_CONSOLE).unwrap().names_console());
    for not in [
        "[instance]\nwork = \"k\"\n",
        "[instance]\nwork = \"k\"\n[tools.console]\nclone = \"/x\"\n",
        "[instance]\nwork = \"k\"\n[tools.console]\nchannel = \"\"\n",
        "[instance]\nwork = \"k\"\n[tools.dashboard]\nchannel = \"k\"\n",
    ] {
        assert!(!Local::parse(not).unwrap().names_console(), "{not}");
    }
}

#[test]
fn nothing_writes_the_dashboard_into_a_config_that_names_a_console() {
    match set(WITH_CONSOLE, "tools.dashboard.url", "https://x") {
        Err(LocalError::DashboardWhereConsole(key)) => assert_eq!(key, "tools.dashboard.url"),
        other => panic!("{other:?}"),
    }
    // The controls: the console's own keys, and the dashboard in a config with no console.
    assert!(set(WITH_CONSOLE, "tools.console.deployed", "abc").is_ok());
    assert!(set(&skeleton("k"), "tools.dashboard.url", "https://x").is_ok());
}

#[test]
fn a_stored_config_cannot_name_a_console_and_carry_a_dashboard() {
    let p = Path::new("/store/k.toml");
    assert!(parse_stored(p, "k", WITH_CONSOLE).is_ok());
    let both = format!("{WITH_CONSOLE}\n[tools.dashboard]\nregistry = [\"k\"]\n");
    assert!(matches!(
        parse_stored(p, "k", &both),
        Err(LocalError::DashboardWhereConsole(_))
    ));
}
