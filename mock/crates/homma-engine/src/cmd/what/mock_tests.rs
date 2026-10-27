//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The mockspace listing: how it is read, bounded and parsed.

use std::process::Command;

use super::sources::mock;

#[cfg(unix)]
fn fake_cargo(dir: &std::path::Path, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("fake-cargo");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[cfg(unix)]
#[test]
fn a_listing_that_writes_more_than_a_pipe_holds_is_read_and_does_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let prog = fake_cargo(
        dir.path(),
        "head -c 400000 /dev/zero | tr '\\0' x >&2\nprintf '    mock lock  seal\\n'",
    );
    let t = std::time::Instant::now();
    let out =
        mock::list(&prog, dir.path(), std::time::Duration::from_secs(30)).expect("the listing");
    assert!(
        t.elapsed() < std::time::Duration::from_secs(20),
        "it waited out its limit"
    );
    assert!(out.contains("mock lock"));
}

#[cfg(unix)]
#[test]
fn a_listing_that_overruns_is_ended_with_what_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("child-pid");
    let prog = fake_cargo(
        dir.path(),
        &format!("sleep 300 &\necho $! > {}\nwait", marker.display()),
    );
    let err = mock::list(&prog, dir.path(), std::time::Duration::from_millis(600)).unwrap_err();
    assert!(err.contains("took longer"), "{err}");
    let pid = std::fs::read_to_string(&marker).unwrap().trim().to_string();
    std::thread::sleep(std::time::Duration::from_millis(200));
    // Dead means gone, or a zombie nobody has reaped yet; `ps` answers on every unix.
    let state = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    assert!(
        state.is_empty() || state.starts_with('Z'),
        "the child {pid} outlived the listing: {state:?}"
    );
}

/// A listing as `cargo mock tools` prints it, cut to a few lines of each section.
const LISTING: &str = "BUILTIN SUBCOMMANDS\n    mock lock                seal the active changelist, advancing the round's phase\n    mock round-id            print a fresh design round id\n    mock help [name]         show this message, or one command in full\n\nPROJECT TOOLS\n    mock ask <question>  check  a question in words against the mock dir, answered with the passages that clear\n";

#[test]
fn the_mockspace_listing_is_read_into_commands() {
    let docs = mock::parse(LISTING, "homma");
    let names: Vec<&str> = docs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["lock", "round-id", "help", "ask"]);
    assert_eq!(docs[0].command, "cargo mock lock");
    assert_eq!(
        docs[0].summary,
        "seal the active changelist, advancing the round's phase"
    );
    assert_eq!(docs[2].command, "cargo mock help [name]");
    let ask = &docs[3];
    assert_eq!(ask.command, "cd homma && cargo mock ask <question>");
    assert_eq!(ask.tags, ["check"]);
    assert_eq!(ask.place, ["homma"]);
    assert!(ask.summary.starts_with("a question in words"));
}

#[test]
fn the_mockspace_repositories_are_those_in_scope() {
    let dir = tempfile::tempdir().unwrap();
    for r in ["a", "b", "c"] {
        std::fs::create_dir(dir.path().join(r)).unwrap();
        std::fs::write(dir.path().join(r).join("mockspace.toml"), "").unwrap();
    }
    std::fs::create_dir(dir.path().join("d")).unwrap();
    assert_eq!(mock::repos(dir.path(), &[]), ["a", "b", "c"]);
    assert_eq!(
        mock::repos(dir.path(), &["b".to_string(), "d".to_string()]),
        ["b"],
        "a repository outside the scope is not asked"
    );
}

#[test]
fn a_repository_where_the_listing_fails_is_named_in_the_footer() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("a")).unwrap();
    std::fs::write(dir.path().join("a/mockspace.toml"), "").unwrap();
    let g = mock::gather(dir.path(), &[]);
    assert!(g.docs.is_empty());
    assert!(
        g.gaps[0]
            .what
            .starts_with("mockspace commands not listed in a:"),
        "{}",
        g.gaps[0].what
    );
}

#[cfg(unix)]
#[test]
fn a_grandchild_holding_the_pipe_after_the_child_exits_does_not_hold_the_listing() {
    let dir = tempfile::tempdir().unwrap();
    let prog = fake_cargo(dir.path(), "(sleep 20 &)\nprintf '    mock lock  seal\\n'");
    let t = std::time::Instant::now();
    let out =
        mock::list(&prog, dir.path(), std::time::Duration::from_secs(30)).expect("the listing");
    assert!(
        t.elapsed() < std::time::Duration::from_secs(10),
        "it waited for the grandchild: {:?}",
        t.elapsed()
    );
    assert!(out.contains("mock lock"));
}
