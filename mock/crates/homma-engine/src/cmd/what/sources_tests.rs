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

#[test]
fn the_scripts_without_a_header_are_counted_in_the_footer() {
    let g = scripts::gather(&workspace());
    match g.gaps.first() {
        Some(gap) => {
            assert_eq!(gap.source, "scripts");
            assert!(
                gap.what
                    .starts_with(&format!("{} scripts under .shared/scripts", gap.count)),
                "{}",
                gap.what
            );
        },
        None => assert!(!g.docs.is_empty()),
    }
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
