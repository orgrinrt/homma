//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The readme's command table documents two binaries, and this is the
//! launcher's half of checking it.
//!
//! The engine's suite checks every span naming an engine command by running the
//! engine. A span naming a command this crate answers before the engine runs is
//! dropped there, and the engine asserts each dropped span is one of the forms
//! the launcher's row for that command writes. So the row is the whole of what
//! reaches this file, and every form in it is checked here: the `workspace` row
//! against `Ask::parse`, and the `config` row by running the binary on a
//! settings directory the test owns.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use homma::workspace::Ask;

const README: &str = include_str!("../../README.md");

/// The table row documenting `homma <name>`, whole.
///
/// The name has to end where the readme's name ends, at a space or the closing
/// backtick, or `work` would find the `workspace` row.
fn row_for(name: &str) -> Option<&'static str> {
    let head = format!("| `homma {name}");
    README.lines().find(|l| {
        l.strip_prefix(&head)
            .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with('`'))
    })
}

/// Every backticked span in a row after its first cell, which is the command
/// itself and not a form of it.
fn forms_in(row: &str) -> Vec<&str> {
    let rest = row.splitn(3, '|').nth(2).unwrap_or_default();
    rest.split('`').skip(1).step_by(2).collect()
}

/// The verb a form starts with.
fn verb_of(form: &str) -> &str {
    form.split_whitespace().next().unwrap_or_default()
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

/// The commands this crate answers without the engine.
fn launchers_commands() -> Vec<&'static str> {
    let mut names: Vec<&str> = homma::TOOL.commands.iter().map(|c| c.name).collect();
    // FIXME: `renki` answers `config` for any tool with settings and keeps the
    // word `pub(crate)` as `config::query::SUBCOMMAND`, so it is written again
    // here. Read it off renki once it exports it.
    if !homma::TOOL.settings.is_empty() {
        names.push("config");
    }
    names
}

/// `homma config <args>`, run on a settings directory under `dir` and from
/// inside `dir`, so neither the person's own file nor a repository's is read.
fn config(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_homma"))
        .arg("config")
        .args(args)
        .current_dir(dir)
        .env(homma::TOOL.config_env(), dir.join("config"))
        .env(homma::TOOL.no_self_update_env(), "1")
        // `edit` runs the editor and then re-reads the file; `true` edits
        // nothing and succeeds, which leaves the re-read as what is checked.
        .env("VISUAL", "true")
        .env_remove("EDITOR")
        .output()
        .expect("the launcher binary would not run")
}

fn said(o: &Output) -> String {
    format!(
        "{}\nstdout: {}\nstderr: {}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn every_command_the_launcher_answers_has_a_row_in_the_readme() {
    let names = launchers_commands();
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

/// The control for the boundary in `row_for`: a prefix of a command's name is
/// not that command.
#[test]
fn a_prefix_of_a_commands_name_finds_no_row() {
    assert!(row_for("workspace").is_some());
    assert!(
        row_for("work").is_none(),
        "`work` found the `workspace` row"
    );
    assert!(row_for("con").is_none(), "`con` found the `config` row");
}

#[test]
fn every_form_the_workspace_row_writes_is_one_the_launcher_parses() {
    let row = row_for("workspace").expect("the readme has no row for `homma workspace`");
    let forms = forms_in(row);
    // The three verbs, each named, so a row that lost its backticks or a
    // reader that stopped finding them cannot pass by checking nothing.
    for verb in ["spawn", "reap", "list"] {
        assert!(
            forms.iter().any(|f| verb_of(f) == verb),
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

/// The `config` row names the verbs the binary takes, all of them and no other.
///
/// Bare `homma config` is refused with the list it takes, which is the live set
/// rather than a copy of it, so a verb renamed or added where the launcher's
/// config query lives reaches this test without anybody editing it.
#[test]
fn the_config_row_names_exactly_the_verbs_the_launcher_takes() {
    let dir = tempfile::tempdir().unwrap();
    let bare = config(dir.path(), &[]);
    assert!(
        !bare.status.success(),
        "bare `homma config` succeeded: {}",
        said(&bare)
    );
    let err = String::from_utf8_lossy(&bare.stderr);
    let listed = err
        .split_once("one of:")
        .unwrap_or_else(|| panic!("the refusal lists no verbs: {}", said(&bare)))
        .1;
    let mut takes: Vec<&str> = listed
        .split(',')
        .map(|v| verb_of(v.trim()))
        .filter(|v| !v.is_empty())
        .collect();
    takes.sort_unstable();

    let row = row_for("config").expect("the readme has no row for `homma config`");
    let mut names: Vec<&str> = forms_in(row).into_iter().map(verb_of).collect();
    names.sort_unstable();
    names.dedup();

    assert!(
        takes.len() >= 5,
        "the refusal listed {takes:?}, so it was not the verb list"
    );
    assert_eq!(
        names, takes,
        "the readme's `config` row and the binary disagree on the verbs"
    );
}

/// Every verb the `config` row names runs, on a settings directory the test
/// owns, with the arguments the verb takes.
///
/// `set` writes every declared key at its own default, which is a claim of its
/// own: a default the file cannot hold is a default nobody can restore.
#[test]
fn every_verb_the_config_row_names_runs() {
    let dir = tempfile::tempdir().unwrap();
    let row = row_for("config").expect("the readme has no row for `homma config`");
    let rows = homma::TOOL.settings;
    assert!(
        !rows.is_empty(),
        "the launcher declares no settings, so `config` is not its"
    );

    let mut ran = 0usize;
    for form in forms_in(row) {
        let verb = verb_of(form);
        let calls: Vec<Vec<&str>> = match verb {
            "path" | "schema" | "edit" => vec![vec![verb]],
            "get" => rows.iter().map(|r| vec![verb, r.key()]).collect(),
            "set" => {
                rows.iter()
                    .map(|r| vec![verb, r.key(), r.default()])
                    .collect()
            },
            other => panic!("the row names `{other}` and this test does not know its arguments"),
        };
        for args in calls {
            let o = config(dir.path(), &args);
            assert!(
                o.status.success(),
                "`homma config {}` failed: {}",
                args.join(" "),
                said(&o)
            );
            ran += 1;
        }
    }
    assert!(ran >= 5, "only {ran} calls ran, so the row lost verbs");
}

/// The control for both tests above: a verb the launcher does not take is
/// refused, so a success there is the verb working rather than the binary
/// accepting anything.
#[test]
fn a_config_verb_the_launcher_does_not_take_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for bad in [&["frob"][..], &["get"], &["get", "no.such.key"], &["set", "workspaces_root"]] {
        let o = config(dir.path(), bad);
        assert!(
            !o.status.success(),
            "`homma config {}` succeeded: {}",
            bad.join(" "),
            said(&o)
        );
    }
}
