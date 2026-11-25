//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A script's header read from its text alone: its summary, its verbs, its body.

use super::sources::scripts;

#[test]
fn a_banner_header_is_a_name_a_summary_and_usage_lines() {
    let text = "#!/usr/bin/env sh\n# ====\n# harvest - which crops are ripe or rotting\n# ====\n#   harvest ripe <bed> <row>   crops ready to pick in a row\n#   harvest rotten  <bed>      crops past saving\n#\n# More prose that explains.\n";
    let d =
        scripts::parse("harvest", ".shared/scripts/chores/harvest", text).expect("a descriptor");
    assert_eq!(d.summary, "which crops are ripe or rotting");
    let verbs: Vec<(&str, &str)> = d
        .verbs
        .iter()
        .map(|v| (v.name.as_str(), v.summary.as_str()))
        .collect();
    assert_eq!(verbs, [
        ("ripe", "crops ready to pick in a row"),
        ("rotten", "crops past saving")
    ]);
    assert_eq!(d.verbs[0].command, ".shared/scripts/chores/harvest ripe");
    assert_eq!(d.body, "More prose that explains.");
}

#[test]
fn a_plain_first_sentence_is_the_summary() {
    let text = "#!/usr/bin/env bash\n# Mend a fence from a plan, after setting what stands aside.\n#\n#   fence-mend.sh <plan> <post>...\n";
    let d = scripts::parse("fence-mend.sh", "p/fence-mend.sh", text).expect("a descriptor");
    assert_eq!(d.name, "fence-mend");
    assert_eq!(
        d.summary,
        "Mend a fence from a plan, after setting what stands aside."
    );
    assert!(d.verbs.is_empty(), "`<plan>` is an argument and not a verb");
}

#[test]
fn a_python_docstring_is_read_with_its_dash() {
    let text = "#!/usr/bin/env python3\n\"\"\"almanac \u{2014} everything due in the garden, in one place.\n\n    almanac   the whole year\n\"\"\"\nimport sys\n";
    let d = scripts::parse("almanac", "p/almanac", text).expect("a descriptor");
    assert_eq!(d.summary, "everything due in the garden, in one place.");
}

#[test]
fn a_block_comment_is_read() {
    let text = "#!/usr/bin/env -S deno run\n/** seed-bank: whether the seeds and their labels agree.\n *\n * The seed bank is a drawer.\n */\nconst x = 1;\n";
    let d = scripts::parse("seed-bank.ts", "p/seed-bank.ts", text).expect("a descriptor");
    assert_eq!(d.summary, "whether the seeds and their labels agree.");
    assert_eq!(d.body, "The seed bank is a drawer.");
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
fn the_first_line_of_prose_is_the_summary_even_when_it_starts_with_the_name() {
    let d = scripts::parse(
        "graft",
        ".shared/scripts/graft",
        "#!/bin/sh\n# graft joins a scion onto the stock\n#   graft check  see whether it would take\n",
    )
    .unwrap();
    assert_eq!(d.summary, "graft joins a scion onto the stock");
    assert_eq!(
        d.verbs.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
        ["check"]
    );
}

#[test]
fn a_column_of_description_after_the_name_is_not_a_verb() {
    let d = scripts::parse(
        "seeds.py",
        "python3 .shared/scripts/garden/seeds.py",
        "#!/usr/bin/env python3\n# seeds.py - the catalogue, scored\n#   seeds.py      every seed variety, its candidates\n#   seeds.py show  one variety\n",
    )
    .unwrap();
    assert_eq!(
        d.verbs.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
        ["show"],
        "`every` follows a column gap and is description"
    );
}

#[test]
fn a_shellcheck_line_is_not_the_summary() {
    let d = scripts::parse(
        "x",
        "x",
        "#!/bin/sh\n# shellcheck source=lib.sh\n# x - does the thing\n",
    )
    .unwrap();
    assert_eq!(d.summary, "does the thing");
}

#[test]
fn a_slash_comment_block_is_a_header() {
    let d = scripts::parse(
        "run.ts",
        "run.ts",
        "#!/usr/bin/env -S deno run\n// run.ts - runs the thing\n// more about it\nconsole.log(1)\n",
    )
    .unwrap();
    assert_eq!(d.summary, "runs the thing");
}

#[test]
fn an_env_shebang_gives_the_interpreter_without_its_flags() {
    assert_eq!(
        scripts::interpreter("#!/usr/bin/env -S deno run").as_deref(),
        Some("deno run")
    );
    assert_eq!(
        scripts::interpreter("#!/usr/bin/env python3").as_deref(),
        Some("python3")
    );
    assert_eq!(scripts::interpreter("#!/bin/bash").as_deref(), Some("bash"));
}

#[test]
fn a_header_line_that_is_only_the_name_is_not_the_summary() {
    let d = scripts::parse(
        "word-frequency.sh",
        "word-frequency.sh",
        "#!/bin/sh\n# word-frequency.sh\n# Counts the words of a text.\n",
    )
    .unwrap();
    assert_eq!(d.summary, "Counts the words of a text.");
}
