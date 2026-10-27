//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::accept_tests::workspace;
use super::doc::Kind;
use super::sources::{corpora, mock, scripts, verbs};

#[test]
fn a_banner_header_is_a_name_a_summary_and_usage_lines() {
    let text = "#!/usr/bin/env nutshell\n# ====\n# sessions - which helper sessions are finished\n# ====\n#   sessions finished <arc> <listing>   ids of idle helpers whose channel says done\n#   sessions blocked  <arc>             ids of helpers stopped\n#\n# More prose that explains.\n";
    let d =
        scripts::parse("sessions", ".shared/scripts/remote/sessions", text).expect("a descriptor");
    assert_eq!(d.summary, "which helper sessions are finished");
    let verbs: Vec<(&str, &str)> = d
        .verbs
        .iter()
        .map(|v| (v.name.as_str(), v.summary.as_str()))
        .collect();
    assert_eq!(verbs, [
        ("finished", "ids of idle helpers whose channel says done"),
        ("blocked", "ids of helpers stopped")
    ]);
    assert_eq!(
        d.verbs[0].command,
        ".shared/scripts/remote/sessions finished"
    );
    assert_eq!(d.body, "More prose that explains.");
}

#[test]
fn a_plain_first_sentence_is_the_summary() {
    let text = "#!/usr/bin/env bash\n# Restore paths from a ref, after copying what they hold aside.\n#\n#   git-restore-safely.sh <ref> <path>...\n";
    let d = scripts::parse("git-restore-safely.sh", "p/git-restore-safely.sh", text)
        .expect("a descriptor");
    assert_eq!(d.name, "git-restore-safely");
    assert_eq!(
        d.summary,
        "Restore paths from a ref, after copying what they hold aside."
    );
    assert!(d.verbs.is_empty(), "`<ref>` is an argument and not a verb");
}

#[test]
fn a_python_docstring_is_read_with_its_dash() {
    let text = "#!/usr/bin/env python3\n\"\"\"agenda \u{2014} everything outstanding, in one place.\n\n    agenda   the whole picture\n\"\"\"\nimport sys\n";
    let d = scripts::parse("agenda", "p/agenda", text).expect("a descriptor");
    assert_eq!(d.summary, "everything outstanding, in one place.");
}

#[test]
fn a_block_comment_is_read() {
    let text = "#!/usr/bin/env -S deno run\n/** memory: whether the corpus and its index agree.\n *\n * Memory is a directory.\n */\nconst x = 1;\n";
    let d = scripts::parse("memory.ts", "p/memory.ts", text).expect("a descriptor");
    assert_eq!(d.summary, "whether the corpus and its index agree.");
    assert_eq!(d.body, "Memory is a directory.");
}

#[test]
fn a_tags_line_adds_tags() {
    let text = "#!/bin/sh\n# does a thing\n# tags: hygiene, disk\n";
    let d = scripts::parse("thing", "p/thing", text).expect("a descriptor");
    assert_eq!(d.tags, ["hygiene", "disk"]);
}

#[test]
fn no_header_is_no_descriptor() {
    assert!(scripts::parse("x", "p/x", "#!/bin/sh\nset -e\necho hi\n").is_none());
    assert!(
        scripts::parse("x", "p/x", "#!/bin/sh\n# ====\n# ====\nset -e\n").is_none(),
        "rules are not prose"
    );
    assert!(scripts::parse("x", "p/x", "").is_none());
}

#[test]
fn only_the_leading_block_is_the_header() {
    let text = "#!/bin/sh\nset -e\n# a comment lower down\n";
    assert!(scripts::parse("x", "p/x", text).is_none());
}

#[test]
fn the_real_sessions_script_has_its_verbs() {
    let g = scripts::gather(&workspace());
    let s = g
        .docs
        .iter()
        .find(|d| d.name == "sessions")
        .expect("the sessions script is read");
    for v in ["finished", "blocked", "live", "due"] {
        assert!(
            s.verbs.iter().any(|x| x.name == v),
            "no verb {v}: {:?}",
            s.verbs
        );
    }
    assert_eq!(s.place, ["remote"]);
}

#[test]
fn tests_and_fixtures_are_not_scripts() {
    let g = scripts::gather(&workspace());
    assert!(!g.docs.is_empty());
    assert!(
        g.docs
            .iter()
            .all(|d| !d.name.contains("_test") && !d.name.contains("_fixture")),
        "a test is listed"
    );
    assert!(g.docs.iter().all(|d| d.name != "test"));
}

fn plant(root: &std::path::Path, rel: &str, text: &str, exec: bool) {
    let p = root.join(".shared/scripts").join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}

#[test]
fn the_scripts_without_a_header_are_counted_in_the_footer() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    plant(
        dir.path(),
        "a/two.py",
        "#!/usr/bin/env python3\n\"\"\"two: does two.\"\"\"\n",
        false,
    );
    plant(dir.path(), "a/bare", "#!/bin/sh\necho hi\n", true);
    plant(dir.path(), "b/bare2.sh", "echo hi\n", false);
    plant(dir.path(), "b/notes.txt", "not a script\n", false);
    plant(dir.path(), "b/one_test.sh", "echo test\n", true);
    let g = scripts::gather(dir.path());
    let mut names: Vec<&str> = g.docs.iter().map(|d| d.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["one", "two"],
        "a non-executable script with an extension is read"
    );
    assert_eq!(g.gaps.len(), 1);
    assert_eq!(
        g.gaps[0].count, 2,
        "the bare executable and the bare .sh are counted, the test and the text are not"
    );
    assert_eq!(
        g.gaps[0].what,
        "2 scripts under .shared/scripts carry no header comment, so they cannot be found by what they do"
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_a_directory_is_not_followed_so_it_cannot_loop() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    let a = dir.path().join(".shared/scripts/a");
    std::os::unix::fs::symlink("..", a.join("up")).unwrap();
    std::os::unix::fs::symlink("..", a.join("up2")).unwrap();
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.len(),
        1,
        "{:?}",
        g.docs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_workspace_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    plant(
        outside.path(),
        "x/secret",
        "#!/bin/sh\n# secret - outside\n",
        true,
    );
    plant(dir.path(), "a/one", "#!/bin/sh\n# one - does one\n", true);
    let a = dir.path().join(".shared/scripts/a");
    std::os::unix::fs::symlink(outside.path().join(".shared/scripts/x"), a.join("ext")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join(".shared/scripts/x/secret"),
        a.join("linked"),
    )
    .unwrap();
    let g = scripts::gather(dir.path());
    assert_eq!(
        g.docs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["one"]
    );
}

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
    // A killed child nobody has reaped yet is a zombie, which is dead.
    let state = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|t| {
            t.rsplit(')')
                .next()
                .and_then(|r| r.split_whitespace().next().map(str::to_string))
        });
    assert!(
        matches!(state.as_deref(), None | Some("Z")),
        "the child {pid} outlived the listing: {state:?}"
    );
}

#[test]
fn no_scripts_directory_is_a_gap_and_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let g = scripts::gather(dir.path());
    assert!(g.docs.is_empty());
    assert_eq!(
        g.gaps[0].what,
        "no .shared/scripts here: scripts not searched"
    );
    assert!(
        corpora::skills(dir.path()).gaps[0]
            .what
            .contains("skills not searched")
    );
    assert!(
        corpora::rules(dir.path()).gaps[0]
            .what
            .contains("rules not searched")
    );
}

#[test]
fn homma_verbs_are_read_from_the_command_line_itself() {
    let g = verbs::gather();
    let rules = g
        .docs
        .iter()
        .find(|d| d.name == "rules")
        .expect("the rules command");
    assert_eq!(rules.kind, Kind::Verb);
    assert!(
        rules
            .verbs
            .iter()
            .any(|v| v.name == "about" && v.command == "homma rules about"),
        "{:?}",
        rules.verbs
    );
    assert!(
        g.docs.iter().any(|d| d.name == "what"),
        "this command finds itself"
    );
    assert!(g.docs.iter().all(|d| d.name != "help"));
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
    assert_eq!(ask.command, "cargo mock ask <question>");
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

#[test]
fn the_real_skills_and_rules_are_read() {
    let ws = workspace();
    let s = corpora::skills(&ws);
    assert!(
        s.docs
            .iter()
            .any(|d| d.name == "work-in-mockspace" && !d.summary.is_empty())
    );
    let r = corpora::rules(&ws);
    assert!(
        r.docs
            .iter()
            .any(|d| d.command == format!("homma rules show {}", d.name) && !d.tags.is_empty())
    );
}

#[cfg(unix)]
#[test]
fn the_real_tools_carry_their_verbs_and_how_to_run_them() {
    let g = super::sources::tools::gather(&workspace());
    let t = g
        .docs
        .iter()
        .find(|d| d.name == "disk-trim")
        .expect("disk-trim");
    assert_eq!(t.command, "homma disk-trim");
    assert!(
        t.verbs
            .iter()
            .any(|v| v.name == "trim" && v.command == "homma disk-trim trim")
    );
    assert!(t.tags.iter().any(|x| x == "hygiene"));
}
