//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::path::PathBuf;

use homma_core::release::sh;
use homma_core::swap::{Swapped, swap_file};

use super::*;

const CATALOGUE: &str = r#"
[org.op]
role = "king"
handle = "op"

[org.ikku]
role = "journeyman"
handle = "ikku"
aliases = ["iida"]
git_email = "ikku@example.invalid"
"#;

/// A bare remote holding `main`, and two workspace clones of it, as two
/// sessions in two containers would have.
struct Two {
    _tmp: tempfile::TempDir,
    a:    PathBuf,
    b:    PathBuf,
    ws:   Workspace,
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = sh::run(cwd, "git", args).unwrap();
    assert!(out.ok(), "git {}: {}", args.join(" "), out.log());
    out.stdout.trim().to_string()
}

fn clone(root: &Path, name: &str) -> PathBuf {
    git(root, &["clone", "--quiet", "remote.git", name]);
    let dir = root.join(name);
    for (k, v) in [
        ("user.email", "t@t"),
        ("user.name", "t"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(&dir, &["config", k, v]);
    }
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
    std::fs::write(seed.join("homma.toml"), CATALOGUE).unwrap();
    git(&seed, &["add", "homma.toml"]);
    git(&seed, &["commit", "--quiet", "-m", "seed"]);
    git(&seed, &["push", "--quiet", "origin", "main"]);
    Two {
        a:    clone(root, "a"),
        b:    clone(root, "b"),
        ws:   Workspace::parse(CATALOGUE).unwrap(),
        _tmp: tmp,
    }
}

fn at(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn take<'a>(session: &'a str, replaces: Option<&'a str>) -> Ask<'a> {
    Ask::Take {
        session,
        job: "identity-lane",
        replaces,
    }
}

#[test]
fn a_take_from_one_clone_is_seen_and_refused_from_the_other() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    let Done::Held(l) = run(&t.a, &t.ws, "ikku", take("s1", None), None, now).unwrap() else {
        panic!("a free identity is taken");
    };
    assert_eq!(
        l.expires,
        at("2026-11-23T14:00:00Z"),
        "`[lease] minutes` by default"
    );
    let err = format!(
        "{:#}",
        run(&t.b, &t.ws, "ikku", take("s2", None), None, now).unwrap_err()
    );
    assert!(err.contains("held by session `s1`"), "{err}");
    let (seen, _) = read(&t.b, &t.ws, "ikku").unwrap();
    assert_eq!(seen.unwrap().session, "s1");
    assert_eq!(
        git(&t.a, &["status", "--porcelain"]),
        "",
        "no working tree is written"
    );
}

#[test]
fn a_named_takeover_lands_and_the_ledger_keeps_both_holders() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    run(&t.a, &t.ws, "ikku", take("s1", None), Some(60), now).unwrap();
    let Done::Held(l) = run(&t.b, &t.ws, "ikku", take("s2", Some("s1")), None, now).unwrap() else {
        panic!("a named takeover writes");
    };
    assert_eq!(l.replaces.as_deref(), Some("s1"));
    let log = git(&t.b, &[
        "log",
        "--format=%s",
        "origin/main",
        "--",
        &record_path(&t.ws, "ikku"),
    ]);
    assert_eq!(
        log,
        "lease: ikku taken over from s1 for identity-lane\nlease: ikku held for identity-lane"
    );
}

#[test]
fn a_release_frees_the_identity_for_the_next_session() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    run(&t.a, &t.ws, "ikku", take("s1", None), None, now).unwrap();
    assert_eq!(
        run(
            &t.a,
            &t.ws,
            "ikku",
            Ask::Release {
                session: "s1",
            },
            None,
            now
        )
        .unwrap(),
        Done::Free
    );
    let (seen, _) = read(&t.b, &t.ws, "ikku").unwrap();
    assert_eq!(seen, None);
    assert!(matches!(
        run(&t.b, &t.ws, "ikku", take("s2", None), None, now).unwrap(),
        Done::Held(_)
    ));
}

#[test]
fn a_take_after_another_landed_is_refused_never_overwritten() {
    // `b` fetched before `a` took the identity, so its tracking ref is stale;
    // the run fetches afresh, and so refuses rather than overwriting.
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    git::fetch_branch(&t.b, "origin", "main").unwrap();
    run(&t.a, &t.ws, "ikku", take("s1", None), None, now).unwrap();
    assert!(run(&t.b, &t.ws, "ikku", take("s2", None), None, now).is_err());
}

#[test]
fn the_king_and_a_stranger_hold_no_lease() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    assert!(run(&t.a, &t.ws, "op", take("s1", None), None, now).is_err());
    assert!(run(&t.a, &t.ws, "nobody", take("s1", None), None, now).is_err());
}

#[test]
fn a_line_says_who_holds_it_and_whom_it_replaced() {
    assert_eq!(describe("ikku", None), "ikku free");
    let l = Lease {
        handle:   "ikku".into(),
        session:  "s2".into(),
        job:      "j".into(),
        taken:    at("2026-11-23T12:00:00Z"),
        expires:  at("2026-11-23T13:00:00Z"),
        replaces: Some("s1".into()),
    };
    assert_eq!(
        describe("ikku", Some(&l)),
        "ikku held by s2 for j since 2026-11-23T12:00:00Z until 2026-11-23T13:00:00Z, replacing s1"
    );
}

#[test]
fn an_alias_takes_the_identity_its_entry_names() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    let Done::Held(l) = run(&t.a, &t.ws, "iida", take("s1", None), None, now).unwrap() else {
        panic!("an alias reaches its entry");
    };
    assert_eq!(l.handle, "ikku");
    let (seen, _) = read(&t.b, &t.ws, "ikku").unwrap();
    assert_eq!(seen.unwrap().session, "s1");
}

#[test]
fn a_name_the_catalogue_lacks_is_refused_even_by_show() {
    let t = two();
    assert!(entry(&t.ws, "ikku").is_ok());
    let err = format!("{:#}", entry(&t.ws, "ikkuu").unwrap_err());
    assert!(err.contains("no handle or alias"), "{err}");
    assert!(entry(&t.ws, "op").is_err());
}

#[test]
fn a_lease_lasts_from_a_minute_to_a_week() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    for bad in [0, 7 * 24 * 60 + 1, u32::MAX] {
        assert!(
            run(&t.a, &t.ws, "ikku", take("s1", None), Some(bad), now).is_err(),
            "{bad}"
        );
    }
    let mut ws = t.ws.clone();
    ws.lease.minutes = 0;
    assert!(
        run(&t.a, &ws, "ikku", take("s1", None), None, now).is_err(),
        "a configured length too"
    );
    assert!(
        run(
            &t.a,
            &t.ws,
            "ikku",
            take("s1", None),
            Some(7 * 24 * 60),
            now
        )
        .is_ok()
    );
}

#[test]
fn a_record_that_cannot_be_read_or_names_another_handle_is_refused_not_free() {
    let t = two();
    let now = at("2026-11-23T12:00:00Z");
    let path = record_path(&t.ws, "ikku");
    let tip = git::fetch_branch(&t.a, "origin", "main").unwrap();
    let Swapped::Landed(tip) = swap_file(
        &t.a,
        "origin",
        "main",
        &tip,
        &path,
        Some("not = [toml"),
        "x",
    )
    .unwrap() else {
        panic!("lands");
    };
    assert!(read(&t.b, &t.ws, "ikku").is_err());
    assert!(
        run(&t.b, &t.ws, "ikku", take("s2", None), None, now).is_err(),
        "never overwritten"
    );
    let other = "handle = \"kide\"\nsession = \"s1\"\njob = \"j\"\ntaken = \"2026-11-23T11:00:00Z\"\nexpires = \"2026-11-23T13:00:00Z\"\n";
    swap_file(&t.a, "origin", "main", &tip, &path, Some(other), "y").unwrap();
    let err = format!("{:#}", read(&t.b, &t.ws, "ikku").unwrap_err());
    assert!(err.contains("names `kide`"), "{err}");
}
