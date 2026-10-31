use std::path::Path;

use super::*;

fn layers(names: &[&str]) -> Layers {
    Layers::new(names.iter().map(|s| s.to_string()).collect()).unwrap()
}

fn runs(h: &Hooks, event: &str) -> Vec<String> {
    h.entries(event)
        .iter()
        .map(|e| e.run().to_string())
        .collect()
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
        "[instance]\nwork = \"x\"\n[[hooks.post-merge]]\nrun = \"local-merge\"\n\
         [[agent.hooks.Stop]]\nrun = \"scripts/stop\"\n",
    );
    let mut hooks = Hooks::default();
    let mut agent = AgentHooks::default();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.local.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    assert_eq!(runs(&hooks, "post-merge"), vec!["local-merge"]);
    assert_eq!(
        agent.iter().map(|(e, r)| (e, r.run())).collect::<Vec<_>>(),
        vec![("Stop", "scripts/stop")]
    );
}

#[test]
fn overlays_append_in_the_order_the_list_gives() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.a.toml",
        "[[hooks.pre-commit]]\nrun = \"from-a\"\n",
    );
    write(
        d.path(),
        "homma.b.toml",
        "[[hooks.pre-commit]]\nrun = \"from-b\"\n",
    );
    let mut hooks: Hooks = toml::from_str("[[pre-commit]]\nrun = \"base\"\n").unwrap();
    let mut agent = AgentHooks::default();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.b.toml", "homma.a.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    assert_eq!(runs(&hooks, "pre-commit"), vec!["base", "from-b", "from-a"]);
    // and the other order gives the other result, which is the control
    let mut hooks: Hooks = toml::from_str("[[pre-commit]]\nrun = \"base\"\n").unwrap();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.a.toml", "homma.b.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    assert_eq!(runs(&hooks, "pre-commit"), vec!["base", "from-a", "from-b"]);
}

#[test]
fn a_named_overlay_that_is_absent_is_skipped() {
    let d = tempfile::tempdir().unwrap();
    let mut hooks = Hooks::default();
    let mut agent = AgentHooks::default();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.local.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    assert_eq!(hooks, Hooks::default());
}

#[test]
fn an_overlay_that_does_not_parse_is_an_error_naming_the_file() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "homma.a.toml", "[[hooks.pre-commit]]\nrun = \n");
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
        "[[hooks.not-a-git-event]]\nrun = \"x\"\n",
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
    write(
        d.path(),
        "homma.stray.toml",
        "[[hooks.pre-commit]]\nrun = \"x\"\n",
    );
    write(d.path(), "homma.local.toml.replaced", "");
    write(d.path(), "homma.local.old.toml", "");
    write(d.path(), "other.toml", "");
    let s = survey(d.path(), &layers(&["homma.local.toml", "homma.team.toml"])).unwrap();
    assert_eq!(s.read, vec!["homma.local.toml"]);
    assert_eq!(s.absent, vec!["homma.team.toml"]);
    assert_eq!(s.strays, vec!["homma.local.old.toml", "homma.stray.toml"]);
    // and a stray is not read: its hook is nowhere in the merged table
    let mut hooks = Hooks::default();
    let mut agent = AgentHooks::default();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.local.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    assert!(hooks.entries("pre-commit").is_empty());
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
            &format!(
                "{MANIFEST}[[hooks.pre-commit]]\nrun = \"base\"\n[[agent.hooks.Stop]]\nrun = \"scripts/base-stop\"\n"
            ),
        );
        write(
            d.path(),
            "homma.local.toml",
            "[instance]\nwork = \"x\"\n[[hooks.pre-commit]]\nrun = \"local\"\n[[agent.hooks.Stop]]\nrun = \"scripts/local-stop\"\n",
        );
        let cfg = load(d.path());
        assert_eq!(runs(&cfg.hooks, "pre-commit"), vec!["base", "local"]);
        let stops: Vec<&str> = cfg.agent.hooks.iter().map(|(_, r)| r.run()).collect();
        assert_eq!(stops, vec!["scripts/base-stop", "scripts/local-stop"]);
    }

    #[test]
    fn the_default_list_is_the_clones_own_file_alone() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "homma.toml", MANIFEST);
        write(
            d.path(),
            "homma.team.toml",
            "[[hooks.pre-commit]]\nrun = \"team\"\n",
        );
        let cfg = load(d.path());
        assert!(
            cfg.hooks.entries("pre-commit").is_empty(),
            "a file nobody named is not read, however it is spelled"
        );
        write(
            d.path(),
            "homma.toml",
            &format!("layers = [\"homma.team.toml\"]\n{MANIFEST}"),
        );
        let cfg = load(d.path());
        assert_eq!(runs(&cfg.hooks, "pre-commit"), vec!["team"]);
    }

    #[test]
    fn a_broken_overlay_is_carried_on_the_config_naming_the_file_and_the_others_still_apply() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "homma.toml",
            &format!("layers = [\"homma.a.toml\", \"homma.b.toml\"]\n{MANIFEST}"),
        );
        write(d.path(), "homma.a.toml", "[[hooks.pre-commit]]\nrun = \n");
        write(
            d.path(),
            "homma.b.toml",
            "[[hooks.pre-commit]]\nrun = \"b\"\n",
        );
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
        assert_eq!(runs(&cfg.hooks, "pre-commit"), vec!["b"]);
        // and a clean load carries none, which is the control
        write(
            d.path(),
            "homma.a.toml",
            "[[hooks.pre-commit]]\nrun = \"a\"\n",
        );
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
fn a_hook_declared_in_the_stored_config_reaches_a_clone_that_names_it() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(crate::local::STORE_DIR)).unwrap();
    std::fs::write(
        d.path().join(crate::local::STORE_DIR).join("k.toml"),
        "[instance]\nwork = \"k\"\n[[hooks.post-merge]]\nrun = \"from-store\"\n",
    )
    .unwrap();
    write(
        d.path(),
        "homma.local.toml",
        "[instance]\nproject = \"k\"\n[[hooks.post-merge]]\nrun = \"from-clone\"\n",
    );
    let mut hooks = Hooks::default();
    let mut agent = AgentHooks::default();
    assert!(
        apply(
            d.path(),
            &layers(&["homma.local.toml"]),
            &mut hooks,
            &mut agent
        )
        .is_empty()
    );
    let mut got = runs(&hooks, "post-merge");
    got.sort();
    // Hook tables are lists, so the clone's own list replaces the project's.
    assert_eq!(got, vec!["from-clone"]);
}

#[test]
fn a_project_the_store_lacks_fails_the_layer_naming_it() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "homma.local.toml",
        "[instance]\nproject = \"gone\"\n",
    );
    let mut hooks = Hooks::default();
    let mut agent = AgentHooks::default();
    let failed = apply(
        d.path(),
        &layers(&["homma.local.toml"]),
        &mut hooks,
        &mut agent,
    );
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0].report().contains("`gone`"),
        "{}",
        failed[0].report()
    );
}
