use std::path::Path;

use super::*;

fn layers(names: &[&str]) -> Layers {
    Layers::new(names.iter().map(|s| s.to_string()).collect()).unwrap()
}

/// The `run` of every agent hook at `event`, in order.
fn runs(agent: &AgentTable, event: &str) -> Vec<String> {
    agent
        .hooks
        .iter()
        .filter(|(e, _)| *e == event)
        .map(|(_, r)| r.run().to_string())
        .collect()
}

fn stop(run: &str) -> String {
    format!("[[agent.hooks.Stop]]\nrun = \"{run}\"\n")
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

#[test]
fn a_hook_declared_only_in_an_overlay_reaches_the_table() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.local.toml",
        &format!("[instance]\nwork = \"x\"\n{}", stop("scripts/stop")),
    );
    let mut agent = AgentTable::default();
    let applied = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent);
    assert!(applied.failed.is_empty());
    assert_eq!(runs(&agent, "Stop"), vec!["scripts/stop"]);
}

#[test]
fn overlays_append_in_the_order_the_list_gives() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", &stop("from-a"));
    write(d.path(), "homma.b.toml", &stop("from-b"));
    let base = || -> AgentTable { toml::from_str(&stop("base").replace("agent.", "")).unwrap() };
    let mut agent = base();
    let applied = apply(
        d.path(),
        &layers(&["homma.b.toml", "homma.a.toml"]),
        &mut agent,
    );
    assert!(applied.failed.is_empty());
    assert_eq!(runs(&agent, "Stop"), vec!["base", "from-b", "from-a"]);
    // and the other order gives the other result, which is the control
    let mut agent = base();
    let applied = apply(
        d.path(),
        &layers(&["homma.a.toml", "homma.b.toml"]),
        &mut agent,
    );
    assert!(applied.failed.is_empty());
    assert_eq!(runs(&agent, "Stop"), vec!["base", "from-a", "from-b"]);
}

#[test]
fn a_named_overlay_that_is_absent_is_skipped() {
    let d = tempfile::tempdir().unwrap();
    let mut agent = AgentTable::default();
    let applied = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent);
    assert!(applied.failed.is_empty() && applied.retired.is_empty());
    assert_eq!(agent, AgentTable::default());
}

#[test]
fn an_overlay_that_does_not_parse_is_an_error_naming_the_file() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", "[[agent.hooks.Stop]]\nrun = \n");
    let err = read(d.path(), "homma.a.toml").unwrap_err();
    assert!(err.to_string().contains("homma.a.toml"), "{err}");
    // the line a verb prints keeps the reader's reason, the position included,
    // which `Display` alone drops
    assert!(
        !err.to_string().contains("line"),
        "control: Display has no cause"
    );
    assert!(err.report().contains("homma.a.toml"), "{}", err.report());
    assert!(err.report().contains("line 2"), "{}", err.report());
    write(
        d.path(),
        "homma.b.toml",
        "[[agent.hooks.NotAnEvent]]\nrun = \"x\"\n",
    );
    let err = read(d.path(), "homma.b.toml").unwrap_err();
    assert!(matches!(err, LayerError::Parse { .. }), "{err}");
    assert!(err.to_string().contains("homma.b.toml"), "{err}");
}

#[test]
fn an_overlay_other_than_the_clones_own_refuses_a_table_nothing_reads() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.a.toml",
        "[forges.github]\nkind = \"github\"\n",
    );
    let err = read(d.path(), "homma.a.toml").unwrap_err();
    assert!(
        matches!(&err, LayerError::Table { table, .. } if table == "forges"),
        "{err}"
    );
    assert!(
        !err.to_string().contains("[hooks]"),
        "an overlay no longer holds git hooks: {err}"
    );
    // the clone's own file carries tables another reader owns, and that is
    // the control: the same text there is not an error from this reader
    write(
        d.path(),
        "homma.local.toml",
        "[forges.github]\nkind = \"github\"\n",
    );
    assert!(read(d.path(), "homma.local.toml").unwrap().is_some());
}

#[test]
fn a_git_hooks_table_left_in_an_overlay_is_named_as_retired_and_never_read() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.a.toml",
        &format!("[[hooks.pre-commit]]\nrun = \"x\"\n{}", stop("kept")),
    );
    let o = read(d.path(), "homma.a.toml").unwrap().unwrap();
    assert_eq!(o.retired, vec![crate::config::Retired {
        table: "hooks".into(),
        file:  "homma.a.toml".into(),
    }]);
    assert_eq!(
        runs(&o.agent, "Stop"),
        vec!["kept"],
        "the rest of the file is read"
    );
    // and the clone's own file the same, through its own reader
    write(
        d.path(),
        "homma.local.toml",
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"x\"\n",
    );
    let mut agent = AgentTable::default();
    let applied = apply(
        d.path(),
        &layers(&["homma.a.toml", "homma.local.toml"]),
        &mut agent,
    );
    assert!(applied.failed.is_empty(), "{:?}", applied.failed);
    let files: Vec<&str> = applied.retired.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(files, vec!["homma.a.toml", "homma.local.toml"]);
    // a file without one names none, which is the control
    write(d.path(), "homma.a.toml", &stop("kept"));
    assert!(
        read(d.path(), "homma.a.toml")
            .unwrap()
            .unwrap()
            .retired
            .is_empty()
    );
}

#[test]
fn agent_carries_hooks_and_nothing_else() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", "[agent]\nrules = []\n");
    let err = read(d.path(), "homma.a.toml").unwrap_err();
    assert!(
        matches!(&err, LayerError::AgentKey { key, .. } if key == "rules"),
        "{err}"
    );
}

#[test]
fn the_survey_names_what_is_read_what_is_absent_and_what_is_never_read() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.toml", "");
    write(d.path(), "homma.local.toml", "");
    write(d.path(), "homma.stray.toml", &stop("x"));
    write(d.path(), "homma.local.toml.replaced", "");
    write(d.path(), "homma.local.old.toml", "");
    write(d.path(), "other.toml", "");
    let s = survey(d.path(), &layers(&["homma.local.toml", "homma.team.toml"])).unwrap();
    assert_eq!(s.read, vec!["homma.local.toml"]);
    assert_eq!(s.absent, vec!["homma.team.toml"]);
    assert_eq!(s.strays, vec!["homma.local.old.toml", "homma.stray.toml"]);
    // and a stray is not read: its hook is nowhere in the merged table
    let mut agent = AgentTable::default();
    let applied = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent);
    assert!(applied.failed.is_empty());
    assert!(agent.hooks.is_empty());
}

#[test]
fn the_clones_own_file_is_unlisted_and_never_a_stray() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.local.toml", "[instance]\nwork = \"x\"\n");
    let s = survey(d.path(), &layers(&["homma.team.toml"])).unwrap();
    assert!(s.own_unlisted);
    assert!(s.strays.is_empty(), "{:?}", s.strays);
    // named, it is read and is neither, which is the control
    let s = survey(d.path(), &layers(&["homma.local.toml"])).unwrap();
    assert!(!s.own_unlisted);
    assert_eq!(s.read, vec!["homma.local.toml"]);
}

mod through_the_config {
    use super::*;
    use crate::Config;

    const MANIFEST: &str = "[workspace]\nname = \"w\"\n";

    fn load(dir: &Path) -> Config {
        Config::from_path(&dir.join("homma.toml")).unwrap()
    }

    #[test]
    fn a_loaded_config_carries_the_merged_tables() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "homma.toml",
            &format!("{MANIFEST}{}", stop("scripts/base-stop")),
        );
        write(
            d.path(),
            "homma.local.toml",
            &format!("[instance]\nwork = \"x\"\n{}", stop("scripts/local-stop")),
        );
        let cfg = load(d.path());
        assert_eq!(runs(&cfg.agent, "Stop"), vec![
            "scripts/base-stop",
            "scripts/local-stop"
        ]);
    }

    #[test]
    fn a_retired_table_in_the_manifest_and_an_overlay_is_carried_on_the_config() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "homma.toml",
            &format!("{MANIFEST}[[hooks.pre-commit]]\nrun = \"base\"\n"),
        );
        write(
            d.path(),
            "homma.local.toml",
            "[instance]\nwork = \"x\"\n[[hooks.pre-commit]]\nrun = \"local\"\n",
        );
        let cfg = load(d.path());
        let files: Vec<&str> = cfg.retired.iter().map(|r| r.file.as_str()).collect();
        assert_eq!(files, vec!["homma.toml", "homma.local.toml"]);
        assert!(cfg.layer_errors.is_empty(), "{:?}", cfg.layer_errors);
    }

    #[test]
    fn the_default_list_is_the_clones_own_file_alone() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "homma.toml", MANIFEST);
        write(d.path(), "homma.team.toml", &stop("team"));
        let cfg = load(d.path());
        assert!(
            cfg.agent.hooks.is_empty(),
            "a file nobody named is not read, however it is spelled"
        );
        write(
            d.path(),
            "homma.toml",
            &format!("layers = [\"homma.team.toml\"]\n{MANIFEST}"),
        );
        let cfg = load(d.path());
        assert_eq!(runs(&cfg.agent, "Stop"), vec!["team"]);
    }

    #[test]
    fn a_broken_overlay_is_carried_on_the_config_naming_the_file_and_the_others_still_apply() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "homma.toml",
            &format!("layers = [\"homma.a.toml\", \"homma.b.toml\"]\n{MANIFEST}"),
        );
        write(d.path(), "homma.a.toml", "[[agent.hooks.Stop]]\nrun = \n");
        write(d.path(), "homma.b.toml", &stop("b"));
        let cfg = load(d.path());
        assert_eq!(cfg.layer_errors.len(), 1, "{:?}", cfg.layer_errors);
        assert!(
            cfg.layer_errors[0].contains("homma.a.toml"),
            "{:?}",
            cfg.layer_errors
        );
        assert!(
            cfg.layer_errors[0].contains("line 2"),
            "the cause reaches the config: {:?}",
            cfg.layer_errors
        );
        assert_eq!(runs(&cfg.agent, "Stop"), vec!["b"]);
        // and a clean load carries none, which is the control
        write(d.path(), "homma.a.toml", &stop("a"));
        assert!(load(d.path()).layer_errors.is_empty());
    }

    #[test]
    fn a_layer_name_that_is_a_path_is_refused_when_the_manifest_loads() {
        let err =
            Config::parse(&format!("layers = [\"../homma.x.toml\"]\n{MANIFEST}")).unwrap_err();
        assert!(err.to_string().contains("does not parse"), "{err}");
    }
}

#[test]
fn a_clones_own_hook_list_replaces_the_projects_for_that_event() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(crate::local::STORE_DIR)).unwrap();
    std::fs::write(
        d.path().join(crate::local::STORE_DIR).join("k.toml"),
        format!("[instance]\nwork = \"k\"\n{}", stop("from-store")),
    )
    .unwrap();
    write(
        d.path(),
        "homma.local.toml",
        &format!("[instance]\nproject = \"k\"\n{}", stop("from-clone")),
    );
    let mut agent = AgentTable::default();
    let applied = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent);
    assert!(applied.failed.is_empty());
    // Hook tables are lists, so the clone's own list replaces the project's.
    assert_eq!(runs(&agent, "Stop"), vec!["from-clone"]);
}

#[test]
fn a_project_the_store_lacks_fails_the_layer_naming_it() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.local.toml",
        "[instance]\nproject = \"gone\"\n",
    );
    let mut agent = AgentTable::default();
    let failed = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent).failed;
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0].report().contains("`gone`"),
        "{}",
        failed[0].report()
    );
}

#[test]
fn a_hook_declared_only_in_the_stored_config_reaches_a_clone_that_names_it() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(crate::local::STORE_DIR)).unwrap();
    std::fs::write(
        d.path().join(crate::local::STORE_DIR).join("k.toml"),
        format!("[instance]\nwork = \"k\"\n{}", stop("from-store")),
    )
    .unwrap();
    write(
        d.path(),
        "homma.local.toml",
        "[instance]\nproject = \"k\"\n",
    );
    let mut agent = AgentTable::default();
    let applied = apply(d.path(), &layers(&["homma.local.toml"]), &mut agent);
    assert!(applied.failed.is_empty());
    assert_eq!(runs(&agent, "Stop"), vec!["from-store"]);
}

fn nudge(name: &str) -> String {
    format!("[[agent.nudges]]\nname = \"{name}\"\nsays = \"x\"\nstrength = \"once\"\n")
}

fn names(agent: &AgentTable) -> Vec<&str> {
    agent.nudges.iter().map(homma_api::Nudge::name).collect()
}

#[test]
fn an_overlays_nudges_come_after_the_ones_read_before_it() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", &nudge("from-a"));
    write(d.path(), "homma.b.toml", &nudge("from-b"));
    let mut agent: AgentTable = toml::from_str(&nudge("base").replace("agent.", "")).unwrap();
    let failed = apply(
        d.path(),
        &layers(&["homma.a.toml", "homma.b.toml"]),
        &mut agent,
    )
    .failed;
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(names(&agent), ["base", "from-a", "from-b"]);
}

#[test]
fn an_overlay_reusing_a_nudge_name_is_refused_whole_and_the_others_still_apply() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.a.toml",
        &format!("{}{}", nudge("base"), stop("scripts/a-stop")),
    );
    write(d.path(), "homma.b.toml", &nudge("from-b"));
    let mut agent: AgentTable = toml::from_str(&nudge("base").replace("agent.", "")).unwrap();
    let failed = apply(
        d.path(),
        &layers(&["homma.a.toml", "homma.b.toml"]),
        &mut agent,
    )
    .failed;
    assert_eq!(failed.len(), 1, "{failed:?}");
    let said = failed[0].report();
    assert!(
        said.contains("homma.a.toml") && said.contains("`base`"),
        "{said}"
    );
    assert_eq!(names(&agent), ["base", "from-b"]);
    assert!(
        agent.hooks.is_empty(),
        "the refused layer's hooks are not added either"
    );

    // the same layer under another name is read whole, which is the control
    write(
        d.path(),
        "homma.a.toml",
        &format!("{}{}", nudge("from-a"), stop("scripts/a-stop")),
    );
    let mut agent: AgentTable = toml::from_str(&nudge("base").replace("agent.", "")).unwrap();
    let applied = apply(
        d.path(),
        &layers(&["homma.a.toml", "homma.b.toml"]),
        &mut agent,
    );
    assert!(applied.failed.is_empty());
    assert_eq!(names(&agent), ["base", "from-a", "from-b"]);
    assert!(!agent.hooks.is_empty());
}

#[test]
fn an_agent_key_beside_hooks_and_nudges_is_refused_by_name() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", &nudge("a"));
    assert!(read(d.path(), "homma.a.toml").is_ok());
    write(d.path(), "homma.a.toml", "[agent.reminders]\nx = 1\n");
    let e = read(d.path(), "homma.a.toml").unwrap_err().to_string();
    assert!(
        e.contains("[agent.reminders]") && e.contains("`nudges`"),
        "{e}"
    );
}
