//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::Config;
use crate::migrate::migrate;

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
// the stored config
// ---------------------------------------------------------------------------

const STORED: &str = "# the stored one\n[instance]\nwork = \"k\"\ntitle = \"stored title\"\nrepos = [\"a\"]\n\n[tools.goal]\nactive = \"false\"\nobjective = \"o\"\n\n[tools.msg]\nas = \"coord\"\n";

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
        Err(LocalError::DashboardWhereConsole(key)) => assert_eq!(key, "tools.dashboard"),
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

const WITH_DASHBOARD: &str = "[instance]\nwork = \"k\"\n\n[tools.dashboard]\nregistry = [\"k\"]\n";

#[test]
fn naming_a_console_beside_a_dashboard_is_refused_by_set() {
    match set(WITH_DASHBOARD, "tools.console.channel", "k") {
        Err(LocalError::DashboardWhereConsole(key)) => assert_eq!(key, "tools.dashboard"),
        other => panic!("{other:?}"),
    }
    // The controls: a console key that does not name one, and the dashboard's own keys.
    assert!(set(WITH_DASHBOARD, "tools.console.deployed", "abc").is_ok());
    assert!(set(WITH_DASHBOARD, "tools.dashboard.url", "https://x").is_ok());
}

// ---------------------------------------------------------------------------
// inheriting a project
// ---------------------------------------------------------------------------

const BASE: &str = "[instance]\nwork = \"k\"\ntitle = \"stored title\"\nrepos = [\"a\", \"b\"]\n\n[tools.goal]\nactive = \"false\"\nobjective = \"o\"\n\n[tools.msg]\nas = \"coord\"\n";

fn with_store(base: &str, local: &str) -> tempfile::TempDir {
    let d = ws();
    std::fs::create_dir_all(d.path().join(STORE_DIR)).unwrap();
    std::fs::write(d.path().join(STORE_DIR).join("k.toml"), base).unwrap();
    std::fs::write(d.path().join(LOCAL_FILE), local).unwrap();
    d
}

fn loaded(d: &tempfile::TempDir) -> Local {
    Local::load(d.path()).unwrap().unwrap()
}

#[test]
fn a_file_of_one_project_line_is_the_whole_stored_config() {
    let d = with_store(BASE, "[instance]\nproject = \"k\"\n");
    let l = loaded(&d);
    assert_eq!(l.instance.work, "k");
    assert_eq!(l.instance.title.as_deref(), Some("stored title"));
    assert_eq!(l.instance.repos, ["a", "b"]);
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(l.tools["msg"]["as"].as_str(), Some("coord"));
    let p = l.provenance.unwrap();
    assert_eq!(p.project, "k");
    assert!(p.local.is_empty() && p.overridden.is_empty() && p.unset.is_empty());
}

#[test]
fn the_file_adds_to_the_base_and_overrides_it() {
    let d = with_store(
        BASE,
        "[instance]\nproject = \"k\"\ntitle = \"mine\"\nrepos = [\"c\"]\n[tools.goal]\nactive = \"true\"\n[tools.extra]\nx = \"1\"\n",
    );
    let l = loaded(&d);
    assert_eq!(l.instance.title.as_deref(), Some("mine"));
    assert_eq!(l.instance.repos, ["c"]);
    assert_eq!(l.tools["goal"]["active"].as_str(), Some("true"));
    assert_eq!(l.tools["goal"]["objective"].as_str(), Some("o"));
    assert_eq!(l.tools["extra"]["x"].as_str(), Some("1"));
    let p = l.provenance.unwrap();
    assert_eq!(p.overridden, [
        "instance.repos",
        "instance.title",
        "tools.goal.active"
    ]);
    assert_eq!(p.local, ["tools.extra.x"]);
    assert_eq!(p.inherited, [
        "instance.work",
        "tools.goal.objective",
        "tools.msg.as"
    ]);
}

#[test]
fn a_change_to_the_store_reaches_a_clone_that_names_it() {
    let d = with_store(BASE, "[instance]\nproject = \"k\"\n");
    assert_eq!(loaded(&d).instance.title.as_deref(), Some("stored title"));
    std::fs::write(
        d.path().join(STORE_DIR).join("k.toml"),
        BASE.replace("stored title", "newer title"),
    )
    .unwrap();
    assert_eq!(loaded(&d).instance.title.as_deref(), Some("newer title"));
}

#[test]
fn the_file_may_set_the_work_itself_and_a_file_without_a_project_must() {
    let d = with_store(BASE, "[instance]\nproject = \"k\"\nwork = \"other\"\n");
    assert_eq!(loaded(&d).instance.work, "other");
    assert!(Local::parse("[instance]\nproject = \"k\"\n").is_ok());
    let e = Local::parse("[instance]\ntitle = \"t\"\n").unwrap_err();
    assert!(e.to_string().contains("work"), "{e}");
}

#[test]
fn unset_removes_an_inherited_value_and_a_table() {
    let d = with_store(
        BASE,
        "[instance]\nproject = \"k\"\nunset = [\"instance.title\", \"tools.msg\"]\n",
    );
    let l = loaded(&d);
    assert_eq!(l.instance.title, None);
    assert!(!l.tools.contains_key("msg"));
    assert!(l.tools.contains_key("goal"));
    assert_eq!(l.provenance.unwrap().unset, ["instance.title", "tools.msg"]);
}

#[test]
fn unset_of_the_mechanism_or_a_malformed_path_is_refused_naming_it() {
    for bad in ["instance.project", "a..b", ""] {
        let d = with_store(
            BASE,
            &format!("[instance]\nproject = \"k\"\nunset = [\"{bad}\"]\n"),
        );
        match Local::load(d.path()) {
            Err(LocalError::Unset(p)) => assert_eq!(p, bad),
            other => panic!("{bad:?}: {other:?}"),
        }
    }
}

#[test]
fn a_project_the_store_lacks_is_an_error_naming_it_and_the_file() {
    let d = ws();
    std::fs::write(
        d.path().join(LOCAL_FILE),
        "[instance]\nproject = \"nope\"\n",
    )
    .unwrap();
    let e = Local::load(d.path()).unwrap_err();
    assert!(matches!(e, LocalError::Project { .. }), "{e:?}");
    let s = e.to_string();
    assert!(s.contains("`nope`") && s.contains("nope.toml"), "{s}");
}

#[test]
fn a_project_name_that_could_leave_the_store_is_refused() {
    let d = ws();
    std::fs::write(
        d.path().join(LOCAL_FILE),
        "[instance]\nproject = \"../x\"\n",
    )
    .unwrap();
    assert!(matches!(
        Local::load(d.path()),
        Err(LocalError::Project { .. })
    ));
}

#[test]
fn a_stored_config_that_names_a_project_is_not_a_base() {
    let d = with_store(
        "[instance]\nwork = \"k\"\nproject = \"j\"\n",
        "[instance]\nproject = \"k\"\n",
    );
    let e = Local::load(d.path()).unwrap_err();
    assert!(e.to_string().contains("names a project"), "{e}");
}

#[test]
fn a_base_whose_work_is_not_its_name_is_refused() {
    let d = with_store(
        "[instance]\nwork = \"zzz\"\n",
        "[instance]\nproject = \"k\"\n",
    );
    assert!(Local::load(d.path()).is_err());
}

#[test]
fn the_console_check_runs_on_the_merged_config_and_unset_resolves_it() {
    let base = "[instance]\nwork = \"k\"\n[tools.dashboard]\nregistry = [\"k\"]\n";
    let console = "[tools.console]\nchannel = \"k\"\n";
    let d = with_store(base, &format!("[instance]\nproject = \"k\"\n{console}"));
    assert!(matches!(
        Local::load(d.path()),
        Err(LocalError::DashboardWhereConsole(_))
    ));
    let d = with_store(
        base,
        &format!("[instance]\nproject = \"k\"\nunset = [\"tools.dashboard\"]\n{console}"),
    );
    let l = loaded(&d);
    assert!(!l.tools.contains_key("dashboard") && l.names_console());
}

#[test]
fn the_merged_table_is_what_the_layers_read() {
    let d = with_store(
        "[instance]\nwork = \"k\"\n[[hooks.post-merge]]\nrun = \"from-store\"\n",
        "[instance]\nproject = \"k\"\n",
    );
    let t = Local::load_table(d.path()).unwrap().unwrap();
    assert_eq!(
        t["hooks"]["post-merge"][0]["run"].as_str(),
        Some("from-store")
    );
}

#[test]
fn a_file_without_a_project_loads_as_written_with_no_provenance() {
    let d = ws();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\nwork = \"k\"\n").unwrap();
    assert_eq!(loaded(&d).provenance, None);
}

// ---------------------------------------------------------------------------
// migrating a copy
// ---------------------------------------------------------------------------

#[test]
fn migrating_a_copy_drops_what_equals_the_store_and_keeps_the_rest() {
    let copy = "# mine\n[instance]\nwork = \"k\"\ntitle = \"stored title\"\nrepos = [\"a\", \"b\"]\n\n[tools.goal]\nactive = \"true\"\nobjective = \"o\"\n\n[tools.msg]\nas = \"coord\"\n\n[tools.console]\ndeployed = \"abc\"\n";
    let m = migrate(copy, BASE, "k").unwrap();
    assert_eq!(m.dropped, [
        "instance.work",
        "instance.title",
        "instance.repos",
        "tools.goal.objective",
        "tools.msg.as"
    ]);
    assert_eq!(m.kept, ["tools.goal.active", "tools.console.deployed"]);
    assert!(m.text.contains("# mine") && m.text.contains("project = \"k\""));
    assert!(!m.text.contains("tools.msg"));
}

#[test]
fn a_migrated_copy_means_what_the_copy_meant() {
    let copy = "[instance]\nwork = \"k\"\ntitle = \"my title\"\nrepos = [\"a\", \"b\"]\n[tools.goal]\nactive = \"true\"\nobjective = \"o\"\n[tools.msg]\nas = \"coord\"\n[tools.recorded]\nsha = \"abc\"\n";
    let m = migrate(copy, BASE, "k").unwrap();
    let d = with_store(BASE, &m.text);
    let before = Local::parse(copy).unwrap();
    let after = loaded(&d);
    assert_eq!(after.instance.work, before.instance.work);
    assert_eq!(after.instance.title, before.instance.title);
    assert_eq!(after.instance.repos, before.instance.repos);
    assert_eq!(after.tools, before.tools);
}

#[test]
fn migrating_a_copy_that_replaced_the_dashboard_with_a_console_unsets_it() {
    let base = "[instance]\nwork = \"k\"\n[tools.dashboard]\nregistry = [\"k\"]\n";
    let copy = "[instance]\nwork = \"k\"\n[tools.console]\nchannel = \"k\"\n";
    let m = migrate(copy, base, "k").unwrap();
    let d = with_store(base, &m.text);
    let l = loaded(&d);
    assert!(l.names_console() && !l.tools.contains_key("dashboard"));
}

#[test]
fn migrating_twice_changes_nothing_more() {
    let copy = "[instance]\nwork = \"k\"\ntitle = \"mine\"\n";
    let once = migrate(copy, BASE, "k").unwrap();
    let twice = migrate(&once.text, BASE, "k").unwrap();
    assert_eq!(once.text, twice.text);
    assert!(twice.dropped.is_empty());
}

#[test]
fn migrate_refuses_text_that_is_not_toml() {
    assert!(matches!(migrate("[[", BASE, "k"), Err(LocalError::Edit(_))));
}

#[test]
fn unsetting_the_work_leaves_none_and_is_refused_naming_the_path() {
    let d = with_store(
        BASE,
        "[instance]\nproject = \"k\"\nunset = [\"instance.work\"]\n",
    );
    match Local::load(d.path()) {
        Err(LocalError::Unset(p)) => assert_eq!(p, "instance.work"),
        other => panic!("{other:?}"),
    }
    // The control: the same file setting its own work is fine.
    let d = with_store(
        BASE,
        "[instance]\nproject = \"k\"\nwork = \"mine\"\nunset = [\"instance.work\"]\n",
    );
    assert_eq!(loaded(&d).instance.work, "mine");
}

#[test]
fn a_copied_array_of_tables_equal_to_the_stores_is_dropped_by_the_migration() {
    let base = "[instance]\nwork = \"k\"\n[[hooks.post-merge]]\nrun = \"a\"\n[[hooks.post-merge]]\nrun = \"b\"\n";
    let m = migrate(base, base, "k").unwrap();
    assert_eq!(m.dropped, ["instance.work", "hooks.post-merge"]);
    assert!(!m.text.contains("post-merge"), "{}", m.text);
    // The control: a copy whose list differs keeps it whole.
    let copy = "[instance]\nwork = \"k\"\n[[hooks.post-merge]]\nrun = \"a\"\n";
    let m = migrate(copy, base, "k").unwrap();
    assert_eq!(m.kept, ["hooks.post-merge"]);
}
