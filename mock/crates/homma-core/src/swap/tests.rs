//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::path::PathBuf;

use super::*;

/// A bare remote with one commit on `main`, and two clones of it, as two
/// sessions in two containers would have.
struct Two {
    _tmp: tempfile::TempDir,
    a:    PathBuf,
    b:    PathBuf,
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = sh::run(cwd, "git", args).unwrap();
    assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
    out.stdout.trim().to_string()
}

fn clone(root: &Path, name: &str) -> PathBuf {
    git(root, &["clone", "--quiet", "remote.git", name]);
    let dir = root.join(name);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    git(&dir, &["config", "core.hooksPath", "/dev/null"]);
    dir
}

fn two() -> Two {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(root, &[
        "init",
        "--quiet",
        "--bare",
        "-b",
        "main",
        "remote.git",
    ]);
    let seed = clone(root, "seed");
    std::fs::write(seed.join("README"), "seed\n").unwrap();
    git(&seed, &["add", "README"]);
    git(&seed, &["commit", "--quiet", "-m", "seed"]);
    git(&seed, &["push", "--quiet", "origin", "main"]);
    let a = clone(root, "a");
    let b = clone(root, "b");
    Two {
        _tmp: tmp,
        a,
        b,
    }
}

const PATH: &str = ".shared/state/leases/inge.toml";

#[test]
fn a_write_lands_on_the_remote_without_touching_the_working_tree() {
    let t = two();
    let tip = git::fetch_branch(&t.a, "origin", "main").unwrap();
    let Swapped::Landed(sha) = swap_file(
        &t.a,
        "origin",
        "main",
        &tip,
        PATH,
        Some("session = \"s1\"\n"),
        "lease: take inge",
    )
    .unwrap() else {
        panic!("a write on the read tip lands");
    };
    assert!(
        !t.a.join(PATH).exists(),
        "the working tree is never written"
    );
    assert_eq!(git(&t.a, &["status", "--porcelain"]), "");
    let fetched = git::fetch_branch(&t.b, "origin", "main").unwrap();
    assert_eq!(fetched, sha);
    assert_eq!(
        git::show(&t.b, "origin/main", PATH).unwrap().as_deref(),
        Some("session = \"s1\"\n")
    );
    assert_eq!(
        git::show(&t.b, "origin/main", "README").unwrap().as_deref(),
        Some("seed\n")
    );
    assert_eq!(
        git::sha(&t.a, "origin/main").unwrap(),
        sha,
        "the tracking ref follows the landing"
    );
}

#[test]
fn a_second_write_built_on_the_same_read_is_refused_as_moved() {
    let t = two();
    let tip_a = git::fetch_branch(&t.a, "origin", "main").unwrap();
    let tip_b = git::fetch_branch(&t.b, "origin", "main").unwrap();
    assert!(matches!(
        swap_file(
            &t.a,
            "origin",
            "main",
            &tip_a,
            PATH,
            Some("session = \"s1\"\n"),
            "a"
        )
        .unwrap(),
        Swapped::Landed(_)
    ));
    assert_eq!(
        swap_file(
            &t.b,
            "origin",
            "main",
            &tip_b,
            PATH,
            Some("session = \"s2\"\n"),
            "b"
        )
        .unwrap(),
        Swapped::Moved
    );
    let after = git::fetch_branch(&t.b, "origin", "main").unwrap();
    assert_eq!(
        git::show(&t.b, &after, PATH).unwrap().as_deref(),
        Some("session = \"s1\"\n")
    );
}

#[test]
fn a_removal_takes_the_file_away_and_keeps_its_history() {
    let t = two();
    let tip = git::fetch_branch(&t.a, "origin", "main").unwrap();
    let Swapped::Landed(first) =
        swap_file(&t.a, "origin", "main", &tip, PATH, Some("x\n"), "take").unwrap()
    else {
        panic!("lands");
    };
    let Swapped::Landed(second) =
        swap_file(&t.a, "origin", "main", &first, PATH, None, "release").unwrap()
    else {
        panic!("lands");
    };
    assert_eq!(git::show(&t.a, &second, PATH).unwrap(), None);
    assert_eq!(
        git::show(&t.a, &second, "README").unwrap().as_deref(),
        Some("seed\n")
    );
    let log = git(&t.a, &["log", "--format=%s", &second, "--", PATH]);
    assert_eq!(log, "release\ntake");
}

#[test]
fn no_scratch_index_is_left_behind() {
    let t = two();
    let tip = git::fetch_branch(&t.a, "origin", "main").unwrap();
    swap_file(&t.a, "origin", "main", &tip, PATH, Some("x\n"), "take").unwrap();
    let left: Vec<_> = std::fs::read_dir(t.a.join(".git"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("homma-swap-index")
        })
        .collect();
    assert!(left.is_empty());
}
