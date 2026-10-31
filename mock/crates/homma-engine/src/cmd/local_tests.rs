//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

fn ws() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("homma.toml"), "[workspace]\nname = \"w\"\n").unwrap();
    d
}

#[test]
fn init_writes_a_file_that_loads_and_ignores_it() {
    let d = ws();
    assert!(init(d.path(), "kenno").unwrap());
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.instance.work, "kenno");
    let ignore = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
    for line in ["/homma.local.toml", "/homma.local.toml.*"] {
        assert!(ignore.lines().any(|l| l == line), "{line} in {ignore}");
    }
}

#[test]
fn init_refuses_to_overwrite() {
    let d = ws();
    init(d.path(), "first").unwrap();
    set(d.path(), "instance.title", "kept").unwrap();
    assert!(init(d.path(), "second").is_err());
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.instance.work, "first");
    assert_eq!(parsed.instance.title.as_deref(), Some("kept"));
}

#[test]
fn init_refuses_where_there_is_no_manifest() {
    let d = tempfile::tempdir().unwrap();
    assert!(init(d.path(), "k").is_err());
    assert!(!d.path().join(LOCAL_FILE).exists());
    assert!(!d.path().join(".gitignore").exists());
}

#[test]
fn init_with_both_lines_already_ignored_reports_no_change() {
    let d = ws();
    std::fs::write(
        d.path().join(".gitignore"),
        "/homma.local.toml\n/homma.local.toml.*\n",
    )
    .unwrap();
    assert!(!init(d.path(), "k").unwrap());
}

#[test]
fn init_with_only_the_file_ignored_adds_the_leftovers() {
    let d = ws();
    std::fs::write(d.path().join(".gitignore"), "/homma.local.toml\n").unwrap();
    assert!(init(d.path(), "k").unwrap());
    assert_eq!(
        std::fs::read_to_string(d.path().join(".gitignore")).unwrap(),
        "/homma.local.toml\n/homma.local.toml.*\n"
    );
}

// ---------------------------------------------------------------------------
// where the file is looked for
// ---------------------------------------------------------------------------

fn cli(args: &[&str]) -> Cli {
    use clap::Parser;
    Cli::try_parse_from(std::iter::once("homma").chain(args.iter().copied())).unwrap()
}

#[test]
fn the_file_is_found_beside_the_manifest_not_at_the_workspace_path() {
    // `workspace.path` pointing elsewhere must not move where the clone's own
    // file is read from, whichever flag names the manifest.
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap();
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(
        root.join("homma.toml"),
        "[workspace]\nname = \"w\"\npath = \"elsewhere\"\n",
    )
    .unwrap();
    std::fs::write(root.join(LOCAL_FILE), "[instance]\nwork = \"beside\"\n").unwrap();
    std::fs::write(elsewhere.join(LOCAL_FILE), "[instance]\nwork = \"root\"\n").unwrap();
    let manifest = root.join("homma.toml");
    for args in [vec!["--config", manifest.to_str().unwrap(), "status"], vec![
        "--dir",
        root.to_str().unwrap(),
        "status",
    ]] {
        let dir = manifest_dir(&cli(&args));
        assert_eq!(dir, root, "{args:?}");
        let l = Local::load(&dir).unwrap().unwrap();
        assert_eq!(l.instance.work, "beside", "{args:?}");
    }
}

#[test]
fn a_relative_manifest_is_resolved_against_the_working_directory() {
    let here = std::env::current_dir().unwrap();
    assert_eq!(
        manifest_dir(&cli(&["--config", "sub/homma.toml", "status"])),
        here.join("sub")
    );
    assert_eq!(manifest_dir(&cli(&["status"])), here);
}

#[test]
fn show_without_a_file_is_none() {
    let d = ws();
    assert!(show(d.path()).unwrap().is_none());
}

#[test]
fn show_returns_the_text_as_written() {
    let d = ws();
    let text = "# mine\n[instance]\nwork = \"k\"\n";
    std::fs::write(d.path().join(LOCAL_FILE), text).unwrap();
    assert_eq!(show(d.path()).unwrap().unwrap().1, text);
}

#[test]
fn show_of_a_malformed_file_is_an_error_not_none() {
    let d = ws();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\n").unwrap();
    assert!(show(d.path()).is_err());
}

#[test]
fn set_without_a_file_refuses_and_writes_nothing() {
    let d = ws();
    assert!(set(d.path(), "instance.work", "k").is_err());
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn set_writes_through_to_the_disk() {
    let d = ws();
    init(d.path(), "k").unwrap();
    set(d.path(), "tools.dashboard.url", "https://x").unwrap();
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert_eq!(parsed.tools["dashboard"]["url"].as_str(), Some("https://x"));
}

#[test]
fn a_refused_set_leaves_the_file_as_it_was() {
    let d = ws();
    init(d.path(), "k").unwrap();
    let before = std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap();
    assert!(set(d.path(), "instance.repos", "not a list").is_err());
    assert!(set(d.path(), "workspace.name", "x").is_err());
    assert_eq!(
        std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap(),
        before
    );
}

#[test]
fn set_repairs_a_file_that_does_not_load() {
    // A file missing its work does not load, and `set` is how it gets it back.
    let d = ws();
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\ntitle = \"t\"\n").unwrap();
    assert!(Local::load(d.path()).is_err());
    set(d.path(), "instance.work", "k").unwrap();
    let l = Local::load(d.path()).unwrap().unwrap();
    assert_eq!(l.instance.work, "k");
    assert_eq!(l.instance.title.as_deref(), Some("t"));
}

#[test]
fn set_leaves_neither_its_lock_nor_its_temporary_file() {
    let d = ws();
    init(d.path(), "k").unwrap();
    set(d.path(), "instance.title", "t").unwrap();
    assert!(set(d.path(), "instance.repos", "refused").is_err());
    let mut left: Vec<String> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, [".gitignore", "homma.local.toml", "homma.toml"]);
}

#[test]
fn a_held_lock_refuses_after_the_wait_and_writes_nothing() {
    let d = ws();
    init(d.path(), "k").unwrap();
    let before = std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap();
    let lock = d.path().join(format!("{LOCAL_FILE}.lock"));
    std::fs::write(&lock, "").unwrap();
    let e = set_waiting(
        d.path(),
        "instance.title",
        "t",
        std::time::Duration::from_millis(50),
    )
    .unwrap_err();
    assert!(e.to_string().contains("is held"), "{e}");
    assert_eq!(
        std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap(),
        before
    );
    // The lock it did not take is not its to remove.
    assert!(lock.exists());
    // The control: the same call lands once the lock is gone.
    std::fs::remove_file(&lock).unwrap();
    set_waiting(
        d.path(),
        "instance.title",
        "t",
        std::time::Duration::from_millis(50),
    )
    .unwrap();
    assert_eq!(
        Local::load(d.path())
            .unwrap()
            .unwrap()
            .instance
            .title
            .as_deref(),
        Some("t")
    );
}

#[test]
fn concurrent_writers_all_land_and_no_reader_sees_a_half() {
    // Many writers, one key each, while a reader loads the file in a loop.
    // Without the lock the read-modify-write loses updates; without the rename
    // a reader catches the file empty between truncate and write.
    let d = ws();
    init(d.path(), "k").unwrap();
    let dir = d.path().to_path_buf();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let (dir, stop) = (dir.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut reads = 0u32;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let l = Local::load(&dir).expect("a reader never sees a broken file");
                assert!(l.is_some(), "a reader never sees the file missing");
                reads += 1;
            }
            reads
        })
    };
    const WRITERS: usize = 96;
    let writers: Vec<_> = (0 .. WRITERS)
        .map(|i| {
            let dir = dir.clone();
            std::thread::spawn(move || set(&dir, &format!("tools.t.k{i}"), &i.to_string()))
        })
        .collect();
    for w in writers {
        w.join().unwrap().unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0, "the reader ran");
    let l = Local::load(&dir).unwrap().unwrap();
    let t = &l.tools["t"];
    for i in 0 .. WRITERS {
        assert_eq!(
            t[&format!("k{i}")].as_str(),
            Some(i.to_string().as_str()),
            "k{i} was lost"
        );
    }
}

// ---------------------------------------------------------------------------
// the stored configs
// ---------------------------------------------------------------------------

const KASKI: &str = "# stored\n[instance]\nwork = \"kaski\"\ntitle = \"the colony sim\"\nrepos = []\n\n[tools.goal]\nactive = \"false\"\n\n[tools.msg]\nas = \"coord\"\n";
const KENNO: &str = "[instance]\nwork = \"kenno\"\n";

fn store(d: &tempfile::TempDir, files: &[(&str, &str)]) {
    let dir = d.path().join(local::STORE_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(format!("{name}.toml")), text).unwrap();
    }
}

fn file(d: &tempfile::TempDir) -> String {
    std::fs::read_to_string(d.path().join(LOCAL_FILE)).unwrap()
}

#[test]
fn install_writes_the_one_project_line_and_no_copy_where_there_is_no_file() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    assert_eq!(
        install(d.path(), "kaski", false).unwrap(),
        Installed::Written
    );
    assert_eq!(file(&d), local::project_skeleton("kaski"));
    let l = Local::load(d.path()).unwrap().unwrap();
    assert_eq!(l.instance.work, "kaski");
    assert_eq!(l.tools["msg"]["as"].as_str(), Some("coord"));
    assert!(
        !std::fs::symlink_metadata(d.path().join(LOCAL_FILE))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn install_ignores_the_file_it_made() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    install(d.path(), "kaski", false).unwrap();
    let ignore = std::fs::read_to_string(d.path().join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "/homma.local.toml"), "{ignore}");
}

#[test]
fn install_leaves_no_lock_or_temporary_file_behind() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    install(d.path(), "kaski", false).unwrap();
    let mut names: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [
        ".gitignore",
        ".shared",
        "homma.local.toml",
        "homma.toml"
    ]);
}

#[test]
fn what_a_tool_records_after_the_install_never_reaches_the_store() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    install(d.path(), "kaski", false).unwrap();
    set(d.path(), "tools.dashboard.published", "abc").unwrap();
    set(d.path(), "tools.console.deployed", "c42e0a9").unwrap();
    assert_eq!(
        std::fs::read_to_string(d.path().join(local::STORE_DIR).join("kaski.toml")).unwrap(),
        KASKI
    );
    assert!(file(&d).contains("published = \"abc\""));
}

#[test]
fn a_link_does_not_survive_the_first_write_which_is_why_install_copies() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    let stored = d.path().join(local::STORE_DIR).join("kaski.toml");
    std::os::unix::fs::symlink(&stored, d.path().join(LOCAL_FILE)).unwrap();
    set(d.path(), "tools.dashboard.published", "abc").unwrap();
    let meta = std::fs::symlink_metadata(d.path().join(LOCAL_FILE)).unwrap();
    assert!(!meta.file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&stored).unwrap(), KASKI);
}

#[test]
fn install_over_a_copy_of_the_same_work_migrates_it_and_keeps_what_differs() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    let copy = "[instance]\nwork = \"kaski\"\ntitle = \"the colony sim\"\n\n[tools.goal]\nactive = \"true\"\n\n[tools.dashboard]\npublished = \"abc\"\n";
    std::fs::write(d.path().join(LOCAL_FILE), copy).unwrap();
    let before = Local::parse(copy).unwrap();
    match install(d.path(), "kaski", false).unwrap() {
        Installed::Migrated {
            dropped,
            kept,
        } => assert!(dropped >= 1 && kept == 2, "{dropped} {kept}"),
        other => panic!("{other:?}"),
    }
    let text = file(&d);
    assert!(
        text.contains("project = \"kaski\"") && !text.contains("work ="),
        "{text}"
    );
    let l = Local::load(d.path()).unwrap().unwrap();
    assert_eq!(l.tools["goal"]["active"].as_str(), Some("true"));
    assert_eq!(l.tools["dashboard"]["published"].as_str(), Some("abc"));
    assert_eq!(l.instance.title, before.instance.title);
    // The control: what the copy lacked, the store now supplies.
    assert_eq!(l.tools["msg"]["as"].as_str(), Some("coord"));
}

#[test]
fn installing_twice_is_the_same_as_once() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    install(d.path(), "kaski", false).unwrap();
    let once = file(&d);
    assert_eq!(
        install(d.path(), "kaski", false).unwrap(),
        Installed::Unchanged
    );
    assert_eq!(file(&d), once);
}

#[test]
fn a_migrated_copy_is_not_migrated_again() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    std::fs::write(d.path().join(LOCAL_FILE), KASKI).unwrap();
    assert!(matches!(
        install(d.path(), "kaski", false).unwrap(),
        Installed::Migrated { .. }
    ));
    let once = file(&d);
    assert_eq!(
        install(d.path(), "kaski", false).unwrap(),
        Installed::Unchanged
    );
    assert_eq!(file(&d), once);
}

#[test]
fn install_over_another_work_refuses_and_touches_nothing() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    install(d.path(), "kenno", false).unwrap();
    let before = file(&d);
    let err = install(d.path(), "kaski", false).unwrap_err().to_string();
    assert!(
        err.contains("`kenno`") && err.contains("`kaski`") && err.contains("--replace"),
        "{err}"
    );
    assert_eq!(file(&d), before);
    assert!(!d.path().join("homma.local.toml.replaced").exists());
}

#[test]
fn replace_moves_the_old_file_aside_whole_and_installs_the_stored_one() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    install(d.path(), "kenno", false).unwrap();
    set(d.path(), "tools.dashboard.published", "abc").unwrap();
    let before = file(&d);
    match install(d.path(), "kaski", true).unwrap() {
        Installed::Replaced(aside) => {
            assert_eq!(aside, d.path().join("homma.local.toml.replaced"));
            assert_eq!(std::fs::read_to_string(aside).unwrap(), before);
        },
        other => panic!("{other:?}"),
    }
    assert_eq!(file(&d), local::project_skeleton("kaski"));
    assert_eq!(
        Local::load(d.path()).unwrap().unwrap().instance.work,
        "kaski"
    );
}

#[test]
fn replace_refuses_to_overwrite_an_earlier_replaced_file() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    install(d.path(), "kenno", false).unwrap();
    std::fs::write(d.path().join("homma.local.toml.replaced"), "older").unwrap();
    assert!(install(d.path(), "kaski", true).is_err());
    assert_eq!(
        std::fs::read_to_string(d.path().join("homma.local.toml.replaced")).unwrap(),
        "older"
    );
    assert!(file(&d).contains("kenno"));
}

#[test]
fn install_over_a_link_refuses_until_replace_and_then_leaves_the_target_alone() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    let stored = d.path().join(local::STORE_DIR).join("kaski.toml");
    std::os::unix::fs::symlink(&stored, d.path().join(LOCAL_FILE)).unwrap();
    assert!(install(d.path(), "kaski", false).is_err());
    assert!(
        std::fs::symlink_metadata(d.path().join(LOCAL_FILE))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(matches!(
        install(d.path(), "kaski", true).unwrap(),
        Installed::Replaced(_)
    ));
    assert!(
        !std::fs::symlink_metadata(d.path().join(LOCAL_FILE))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&stored).unwrap(), KASKI);
}

#[test]
fn install_over_a_file_that_does_not_parse_refuses_without_replace() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    std::fs::write(d.path().join(LOCAL_FILE), "[instance]\n").unwrap();
    assert!(install(d.path(), "kaski", false).is_err());
    assert_eq!(file(&d), "[instance]\n");
    assert!(matches!(
        install(d.path(), "kaski", true).unwrap(),
        Installed::Replaced(_)
    ));
}

#[test]
fn an_unknown_name_is_refused_with_the_names_there_are() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    let err = install(d.path(), "nope", false).unwrap_err().to_string();
    assert!(err.contains("kaski, kenno"), "{err}");
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn a_name_that_could_leave_the_store_is_refused() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    std::fs::write(
        d.path().join("outside.toml"),
        "[instance]\nwork = \"../../outside\"\n",
    )
    .unwrap();
    for name in ["../../outside", "a/b", "", ".hidden", "k k"] {
        assert!(install(d.path(), name, false).is_err(), "{name:?}");
    }
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn a_stored_file_whose_work_is_not_its_name_is_refused() {
    let d = ws();
    store(&d, &[("kenno", KASKI)]);
    let err = install(d.path(), "kenno", false).unwrap_err().to_string();
    assert!(err.contains("`kenno`") && err.contains("`kaski`"), "{err}");
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn install_without_a_manifest_refuses() {
    let d = tempfile::tempdir().unwrap();
    store(&d, &[("kaski", KASKI)]);
    assert!(install(d.path(), "kaski", false).is_err());
    assert!(!d.path().join(LOCAL_FILE).exists());
}

#[test]
fn list_names_the_stored_configs_sorted_and_marks_the_installed_one() {
    let d = ws();
    store(&d, &[("kenno", KENNO), ("kaski", KASKI)]);
    std::fs::write(d.path().join(local::STORE_DIR).join("README.md"), "x").unwrap();
    install(d.path(), "kenno", false).unwrap();
    let (entries, installed) = list(d.path()).unwrap().unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["kaski", "kenno"]);
    assert_eq!(installed.as_deref(), Some("kenno"));
    assert_eq!(entries[0].title.as_deref(), Some("the colony sim"));
}

#[test]
fn list_with_nothing_installed_marks_none_and_without_a_store_is_none() {
    let d = ws();
    assert!(list(d.path()).unwrap().is_none());
    store(&d, &[("kaski", KASKI)]);
    assert_eq!(list(d.path()).unwrap().unwrap().1, None);
}

#[test]
fn list_shows_an_unusable_stored_file_instead_of_hiding_it() {
    let d = ws();
    store(&d, &[
        ("kaski", KASKI),
        ("broken", "[instance]\n"),
        ("kenno", KASKI),
    ]);
    let (entries, _) = list(d.path()).unwrap().unwrap();
    let bad: Vec<_> = entries
        .iter()
        .filter(|e| e.error.is_some())
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(bad, ["broken", "kenno"]);
}

#[test]
fn list_shows_a_name_that_install_would_refuse_with_the_reason() {
    let d = ws();
    store(&d, &[
        ("kaski", KASKI),
        ("a.b", "[instance]\nwork = \"a.b\"\n"),
    ]);
    let (entries, _) = list(d.path()).unwrap().unwrap();
    let by = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
    assert_eq!(by("kaski").error, None);
    let reason = by("a.b").error.as_deref().expect("listed with its reason");
    assert!(reason.contains("not a stored name"), "{reason}");
    // The control: what `install` says of the same name is what `list` shows.
    let refused = install(d.path(), "a.b", false).unwrap_err().to_string();
    assert!(refused.contains("not a stored name"), "{refused}");
}

#[cfg(unix)]
#[test]
fn list_shows_a_file_whose_name_is_not_utf8_instead_of_skipping_it() {
    use std::os::unix::ffi::OsStrExt;
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    let odd = std::ffi::OsStr::from_bytes(b"a\xffb.toml");
    std::fs::write(
        d.path().join(local::STORE_DIR).join(odd),
        "[instance]\nwork = \"x\"\n",
    )
    .unwrap();
    let (entries, _) = list(d.path()).unwrap().unwrap();
    assert_eq!(entries.len(), 2, "{entries:?}");
    let bad = entries.iter().find(|e| e.error.is_some()).unwrap();
    assert!(
        bad.name.contains("a") && bad.name.contains("b"),
        "{}",
        bad.name
    );
    assert!(
        bad.error.as_deref().unwrap().contains("UTF-8"),
        "{:?}",
        bad.error
    );
}

#[test]
fn a_replace_whose_write_fails_leaves_the_old_file_where_it_was() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    install(d.path(), "kenno", false).unwrap();
    let before = file(&d);
    // A directory where the temporary file goes, so writing it fails.
    std::fs::create_dir(
        d.path()
            .join(format!("{LOCAL_FILE}.{}.tmp", std::process::id())),
    )
    .unwrap();
    assert!(install(d.path(), "kaski", true).is_err());
    assert_eq!(file(&d), before, "the old file is still the file");
    assert!(
        !d.path().join("homma.local.toml.replaced").exists(),
        "nothing was moved aside for a write that did not happen"
    );
}

const KASKI_CONSOLE: &str =
    "[instance]\nwork = \"kaski\"\n\n[tools.console]\nchannel = \"kaski\"\ndeploy = \"x\"\n";

#[test]
fn install_writes_the_console_form_and_refuses_a_stored_dashboard_beside_it() {
    let d = ws();
    store(&d, &[("kaski", KASKI_CONSOLE)]);
    assert_eq!(
        install(d.path(), "kaski", false).unwrap(),
        Installed::Written
    );
    assert_eq!(file(&d), local::project_skeleton("kaski"));
    let (parsed, _) = show(d.path()).unwrap().unwrap();
    assert!(parsed.names_console());

    // The control: the same store with a dashboard table beside the console is refused,
    // and the clone that holds the good file is left as it was.
    let both = format!("{KASKI_CONSOLE}\n[tools.dashboard]\nregistry = [\"kaski\"]\n");
    store(&d, &[("kaski", &both)]);
    let err = install(d.path(), "kaski", true).unwrap_err().to_string();
    assert!(
        err.contains("tools.dashboard") && err.contains("console"),
        "{err}"
    );
    assert_eq!(file(&d), local::project_skeleton("kaski"));
}

#[test]
fn a_tool_cannot_record_a_dashboard_into_a_clone_that_names_a_console() {
    let d = ws();
    store(&d, &[("kaski", KASKI_CONSOLE)]);
    install(d.path(), "kaski", false).unwrap();
    assert!(set(d.path(), "tools.dashboard.published", "abc").is_err());
    assert!(set(d.path(), "tools.console.deployed", "abc").is_ok());
    assert!(!file(&d).contains("dashboard"));
}

#[test]
fn install_into_a_clone_keeping_its_dashboard_does_not_add_a_console() {
    let d = ws();
    let old = "[instance]\nwork = \"kaski\"\n\n[tools.dashboard]\nregistry = [\"kaski\"]\n";
    std::fs::write(d.path().join(LOCAL_FILE), old).unwrap();
    store(&d, &[("kaski", KASKI_CONSOLE)]);
    let err = install(d.path(), "kaski", false).unwrap_err().to_string();
    assert!(
        err.contains("tools.dashboard") && err.contains("console"),
        "{err}"
    );
    assert_eq!(file(&d), old);
}

#[test]
fn show_gives_the_merged_document_while_the_text_stays_as_written() {
    let d = ws();
    store(&d, &[("kaski", KASKI)]);
    install(d.path(), "kaski", false).unwrap();
    let (parsed, text) = show(d.path()).unwrap().unwrap();
    assert_eq!(text, local::project_skeleton("kaski"));
    assert_eq!(parsed.instance.title.as_deref(), Some("the colony sim"));
    assert_eq!(parsed.instance.project.as_deref(), Some("kaski"));
    let doc = serde_json::to_value(&parsed).unwrap();
    assert_eq!(doc["tools"]["msg"]["as"], "coord");
}

#[test]
fn list_marks_the_project_a_file_names_and_the_work_a_copy_is_for() {
    let d = ws();
    store(&d, &[("kaski", KASKI), ("kenno", KENNO)]);
    install(d.path(), "kenno", false).unwrap();
    assert_eq!(list(d.path()).unwrap().unwrap().1.as_deref(), Some("kenno"));
    std::fs::write(d.path().join(LOCAL_FILE), KASKI).unwrap();
    assert_eq!(list(d.path()).unwrap().unwrap().1.as_deref(), Some("kaski"));
}
