use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use homma_api::HookEntry;

use super::*;
use crate::hooks::install::{HookError, Reach, install};

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let out = sh::run(d.path(), "git", &["init", "--quiet", "-b", "main"]).unwrap();
    assert!(out.ok());
    for (k, v) in [("user.name", "t"), ("user.email", "t@t")] {
        assert!(sh::run(d.path(), "git", &["config", k, v]).unwrap().ok());
    }
    d
}

fn table(events: &[&str]) -> Hooks {
    let mut declared = BTreeMap::new();
    for e in events {
        declared.insert(e.to_string(), vec![
            HookEntry::new("true", Vec::new()).unwrap(),
        ]);
    }
    Hooks::new(declared).unwrap()
}

fn executable(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// A stand-in for git-shook that records what it was called with and
/// registers, for every `[hooks.<event>]` in the local manifest, the entry
/// file the real one would write.
fn fake_shook(bin: &Path, log: &Path) {
    executable(
        bin,
        "git-shook",
        &format!(
            "#!/bin/sh\nprintf '%s|' \"$*\" >> '{log}'\npwd >> '{log}'\n\
             g=$(git rev-parse --path-format=absolute --git-common-dir)\n\
             for e in $(sed -n 's/^\\[hooks\\.\\(.*\\)\\]$/\\1/p' \"$g/shook/manifests/homma.toml\"); do\n\
             mkdir -p \"$g/shook/entries/$e\"; : > \"$g/shook/entries/$e/homma\"\ndone\n",
            log = log.display()
        ),
    );
}

#[test]
fn the_manifest_names_every_event_the_table_does_with_the_command_that_runs_it() {
    let text = manifest(&table(&["pre-commit", "post-merge"]));
    let parsed: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(parsed["name"].as_str(), Some("homma"));
    let hooks = parsed["hooks"].as_table().unwrap();
    assert_eq!(
        hooks.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["post-merge", "pre-commit", "pre-push"],
        "the gate's event is in every table"
    );
    for (event, row) in hooks {
        let run: Vec<&str> = row["run"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(run, vec!["homma", "hook", "run", event.as_str()]);
    }
}

#[test]
fn an_install_writes_the_manifest_and_has_git_shook_register_it() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("log");
    fake_shook(bin.path(), &log);

    let i = install_with(d.path(), &table(&["post-merge"]), Some(bin.path())).unwrap();

    let common = d.path().canonicalize().unwrap().join(".git");
    assert_eq!(
        std::fs::read_to_string(common.join("shook/manifests/homma.toml")).unwrap(),
        manifest(&table(&["post-merge"]))
    );
    let called = std::fs::read_to_string(&log).unwrap();
    assert!(
        called.starts_with("install --yes --only homma|"),
        "the consent covers homma's manifest only: {called}"
    );
    assert!(
        called
            .trim_end()
            .ends_with(d.path().canonicalize().unwrap().to_str().unwrap()),
        "run from the repository: {called}"
    );
    let events: Vec<&str> = i.written.iter().map(|w| w.event.as_str()).collect();
    assert_eq!(events, vec!["post-merge", "pre-push"]);
    for w in &i.written {
        assert_eq!(
            w.path,
            common.join("shook/entries").join(&w.event).join("homma")
        );
        assert_eq!(w.reach, Reach::Shook(w.path.clone()));
    }
    assert!(i.reached());
}

#[test]
fn registering_removes_the_entrypoint_this_tool_wrote_and_nobody_elses() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    fake_shook(bin.path(), &bin.path().join("log"));
    let own = crate::hooks::hooks_dir(d.path()).unwrap();
    std::fs::create_dir_all(&own).unwrap();
    // ours, byte for byte, at one event; someone else's at another
    std::fs::write(own.join("post-merge"), crate::hooks::script("post-merge")).unwrap();
    std::fs::write(own.join("pre-push"), "#!/bin/sh\necho theirs\n").unwrap();

    install_with(d.path(), &table(&["post-merge"]), Some(bin.path())).unwrap();

    assert!(
        !own.join("post-merge").exists(),
        "the stale entrypoint of ours is gone"
    );
    assert!(
        own.join("pre-push").is_file(),
        "a hook that is not ours stays"
    );
}

#[test]
fn the_copy_git_shook_kept_of_the_entrypoint_goes_too_and_a_foreign_one_stays() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    fake_shook(bin.path(), &bin.path().join("log"));
    let registry = d.path().canonicalize().unwrap().join(".git/shook");
    // what git-shook leaves when it adopts a hook: the copy and the entry that
    // runs it. Ours at post-merge, somebody else's at pre-push.
    for (event, text) in [
        ("post-merge", crate::hooks::script("post-merge")),
        ("pre-push", "#!/bin/sh\necho theirs\n".to_string()),
    ] {
        std::fs::create_dir_all(registry.join("kept")).unwrap();
        std::fs::create_dir_all(registry.join("entries").join(event)).unwrap();
        std::fs::write(registry.join("kept").join(event), text).unwrap();
        std::fs::write(
            registry.join("entries").join(event).join("00-existing"),
            "x",
        )
        .unwrap();
    }

    install_with(d.path(), &table(&["post-merge"]), Some(bin.path())).unwrap();

    assert!(!registry.join("kept/post-merge").exists());
    assert!(!registry.join("entries/post-merge/00-existing").exists());
    assert!(
        registry.join("entries/post-merge/homma").is_file(),
        "the registered entry itself stays"
    );
    assert!(
        registry.join("kept/pre-push").is_file(),
        "a foreign kept hook stays"
    );
    assert!(registry.join("entries/pre-push/00-existing").is_file());
}

#[test]
fn install_routes_to_git_shook_only_where_it_holds_the_hooks_path() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("log");
    fake_shook(bin.path(), &log);
    let shook_dir = hooks_dir(d.path()).unwrap();
    std::fs::create_dir_all(&shook_dir).unwrap();
    assert!(
        sh::run(d.path(), "git", &[
            "config",
            "core.hooksPath",
            shook_dir.to_str().unwrap()
        ])
        .unwrap()
        .ok()
    );

    let i = crate::hooks::install::install_on(d.path(), &table(&["post-merge"]), Some(bin.path()))
        .unwrap();

    assert!(log.is_file(), "git-shook was called");
    assert!(i.written.iter().all(|w| matches!(w.reach, Reach::Shook(_))));
}

#[test]
fn git_shook_registering_less_than_the_manifest_declares_is_an_error_naming_the_event() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    executable(bin.path(), "git-shook", "#!/bin/sh\nexit 0\n");
    let err = install_with(d.path(), &table(&[]), Some(bin.path())).unwrap_err();
    assert!(
        matches!(&err, HookError::Shook(s) if s.contains("registered nothing for pre-push")),
        "{err}"
    );
}

#[test]
fn a_refusal_gives_the_manifest_back_as_it_was() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    executable(bin.path(), "git-shook", "#!/bin/sh\nexit 1\n");
    let file = d
        .path()
        .canonicalize()
        .unwrap()
        .join(".git/shook/manifests/homma.toml");

    // none before: none after
    assert!(install_with(d.path(), &table(&["post-merge"]), Some(bin.path())).is_err());
    assert!(
        !file.exists(),
        "a refusal left a manifest where there was none"
    );

    // one before: the same text after
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "name = \"homma\"\n# old\n").unwrap();
    assert!(install_with(d.path(), &table(&["post-merge"]), Some(bin.path())).is_err());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "name = \"homma\"\n# old\n"
    );
}

#[test]
fn a_git_shook_that_refuses_is_reported_with_what_it_said() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    executable(
        bin.path(),
        "git-shook",
        "#!/bin/sh\necho 'git-shook: no thanks' >&2\nexit 1\n",
    );
    let err = install_with(d.path(), &table(&[]), Some(bin.path())).unwrap_err();
    assert!(err.to_string().contains("no thanks"), "{err}");
}

#[test]
fn git_without_the_subcommand_says_git_shook_is_not_installed() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    // git itself, found through the real PATH, answers for a subcommand it
    // has no program for; the fake dir carries no git-shook, so on a machine
    // that has none either this is what a missing install looks like
    if sh::run(d.path(), "git", &["shook", "version"])
        .map(|o| o.ok())
        .unwrap_or(false)
    {
        return;
    }
    let err = install_with(d.path(), &table(&[]), Some(bin.path())).unwrap_err();
    assert!(
        err.to_string().contains("git-shook is not installed"),
        "{err}"
    );
}

#[test]
fn install_writes_entrypoints_and_registers_nothing_where_git_shook_holds_no_key() {
    let d = repo();
    // an empty table would make the all-Direct assertion vacuous, so it names
    // an event, and the entrypoint it writes is the control
    let i = install(d.path(), &table(&["pre-commit"])).unwrap();
    assert!(i.written.iter().any(|w| w.event == "pre-commit"));
    assert!(i.written.iter().all(|w| w.reach == Reach::Direct));
    assert!(!d.path().join(".git/shook").exists());
}

/// The end to end: the real git-shook, a layer-only hook, and a commit that
/// runs it. Needs `GIT_SHOOK` pointing at the git-shook script, since git-shook
/// is a separate program the machine may not have; without it the test says so
/// and returns.
#[test]
fn a_hook_declared_only_in_a_layer_is_registered_and_runs_on_a_real_commit() {
    let Some(script) = std::env::var_os("GIT_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: GIT_SHOOK names no git-shook script");
        return;
    };
    let shook_dir = script.parent().unwrap().to_path_buf();
    let d = repo();
    // the layer: a hook the manifest does not have, in a file only this clone has
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    std::fs::write(
        d.path().join("homma.local.toml"),
        "[instance]\nwork = \"x\"\n[[hooks.pre-commit]]\nrun = \"true\"\n",
    )
    .unwrap();
    let cfg = crate::Config::from_path(&d.path().join("homma.toml")).unwrap();
    assert_eq!(
        cfg.hooks.entries("pre-commit").len(),
        1,
        "the layer reached the table"
    );

    let i = install_with(d.path(), &cfg.hooks, Some(&shook_dir)).unwrap();
    assert!(i.reached());
    let events: Vec<&str> = i.written.iter().map(|w| w.event.as_str()).collect();
    assert_eq!(events, vec!["pre-commit", "pre-push"]);

    // a homma that records how it was called, ahead of any on the machine
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("called");
    executable(
        bin.path(),
        "homma",
        &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    );
    std::fs::write(d.path().join("f"), "x").unwrap();
    let path = format!(
        "{}:{}:{}",
        bin.path().display(),
        shook_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    assert!(
        sh::run_with_env(d.path(), "git", &["add", "f"], &[("PATH", &path)])
            .unwrap()
            .ok()
    );
    let committed = sh::run_with_env(d.path(), "git", &["commit", "-qm", "one"], &[(
        "PATH", &path,
    )])
    .unwrap();
    assert!(committed.ok(), "{}", committed.log());
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        "hook run pre-commit",
        "the dispatcher ran homma for the event the layer declared"
    );

    // and the registration agrees with the declarations
    let doctor =
        sh::run_with_env(d.path(), "git", &["shook", "doctor"], &[("PATH", &path)]).unwrap();
    assert!(doctor.ok(), "{}", doctor.log());
}
