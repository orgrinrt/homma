//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The readme's command table documents two binaries, and this is the
//! launcher's half of checking it.
//!
//! The engine's suite checks every row by running the engine, and leaves out
//! the commands this crate answers before the engine runs, reading which those
//! are off `TOOL.commands`. So a row for one of them is checked here or
//! nowhere: that the row exists, and that every form it writes in backticks is
//! one `Ask::parse` takes.

use std::ffi::OsString;

use homma::workspace::Ask;

const README: &str = include_str!("../../README.md");

/// The table row documenting `homma <name>`, whole.
fn row_for(name: &str) -> Option<&'static str> {
    let head = format!("| `homma {name}");
    README.lines().find(|l| l.starts_with(&head))
}

/// Every backticked span in a row after its first cell, which is the command
/// itself and not a form of it.
fn forms_in(row: &str) -> Vec<&str> {
    let rest = row.splitn(3, '|').nth(2).unwrap_or_default();
    rest.split('`').skip(1).step_by(2).collect()
}

/// A written form as the words a person would type, in the two readings its
/// optional groups allow: every `[...]` dropped, and every one filled.
///
/// A `<placeholder>` stands for a value and becomes its own name, which is a
/// valid slug and a valid repository alike. A trailing `...` is repetition and
/// is dropped.
fn typed(form: &str) -> [Vec<OsString>; 2] {
    let mut short = Vec::new();
    let mut full = Vec::new();
    let mut depth = 0usize;
    for word in form.split_whitespace() {
        let opens = word.matches('[').count();
        let closes = word.matches(']').count();
        let bare = word
            .trim_matches(|c| c == '[' || c == ']')
            .trim_matches(|c| c == '<' || c == '>');
        if !bare.is_empty() && bare != "..." {
            let v = OsString::from(bare);
            if depth == 0 && opens == 0 {
                short.push(v.clone());
            }
            full.push(v);
        }
        depth = (depth + opens).saturating_sub(closes);
    }
    [short, full]
}

#[test]
fn every_command_the_launcher_answers_has_a_row_in_the_readme() {
    let mut names: Vec<&str> = homma::TOOL.commands.iter().map(|c| c.name).collect();
    // FIXME: `renki` answers `config` for any tool with settings and keeps the
    // word `pub(crate)` as `config::query::SUBCOMMAND`, so it is written again
    // here. Read it off renki once it exports it.
    if !homma::TOOL.settings.is_empty() {
        names.push("config");
    }
    assert!(
        !names.is_empty(),
        "the launcher answers no command of its own, so this checked nothing"
    );
    let missing: Vec<&&str> = names.iter().filter(|n| row_for(n).is_none()).collect();
    assert!(
        missing.is_empty(),
        "the launcher answers commands the readme's table has no row for: {missing:?}"
    );
}

#[test]
fn every_form_the_workspace_row_writes_is_one_the_launcher_parses() {
    let row = row_for("workspace").expect("the readme has no row for `homma workspace`");
    let forms = forms_in(row);
    // The three verbs, each named, so a row that lost its backticks or a
    // reader that stopped finding them cannot pass by checking nothing.
    for verb in ["spawn", "reap", "list"] {
        assert!(
            forms
                .iter()
                .any(|f| f.split_whitespace().next() == Some(verb)),
            "the row no longer writes a form for `{verb}`: {forms:?}"
        );
    }
    let mut wrong = Vec::new();
    for form in &forms {
        for args in typed(form) {
            if let Err(e) = Ask::parse(&args) {
                wrong.push(format!("`{form}` typed as {args:?}: {e}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Bare is a form too, and the row's first cell writes it.
    assert_eq!(Ask::parse(&[]), Ok(Ask::Bare));
}

/// The control for the check above: a form the launcher does not take is
/// reported, and the reading of optional groups is the one the test relies on.
#[test]
fn a_form_the_launcher_does_not_take_is_refused() {
    let [short, full] = typed("reap [<slug>]");
    assert_eq!(short, vec![OsString::from("reap")]);
    assert_eq!(full, vec![OsString::from("reap"), OsString::from("slug")]);

    let [short, full] = typed("spawn <slug> [owner/name ...]");
    assert_eq!(short, vec![OsString::from("spawn"), OsString::from("slug")]);
    assert_eq!(full, vec![
        OsString::from("spawn"),
        OsString::from("slug"),
        OsString::from("owner/name"),
    ]);

    for bad in ["prune", "list everything", "spawn", "reap a b"] {
        let [short, _] = typed(bad);
        assert!(
            Ask::parse(&short).is_err(),
            "`{bad}` parsed, so a wrong form in the row would pass"
        );
    }
}
