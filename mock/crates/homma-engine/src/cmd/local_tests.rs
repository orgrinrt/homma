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
