//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The readme shows a manifest and a table of commands. Both are claims about
//! this crate, so both are parsed against it here rather than trusted.
//!
//! A readme example that no longer parses is the first thing a new reader
//! meets and the last thing anybody rereads, so it gets a test of its own.

use assert_cmd::Command;
use homma_core::config::Config;

fn bin() -> Command {
    Command::cargo_bin("homma-engine").expect("binary built")
}

const README: &str = include_str!("../../../../README.md");

/// Every fenced block in the readme tagged with `lang`.
fn fenced_blocks(lang: &str) -> Vec<String> {
    let open = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut rest = README;
    while let Some(at) = rest.find(&open) {
        let body = &rest[at + open.len() ..];
        let Some(nl) = body.find('\n') else { break };
        let body = &body[nl + 1 ..];
        let Some(end) = body.find("```") else { break };
        blocks.push(body[.. end].to_string());
        rest = &body[end ..];
    }
    blocks
}

#[test]
fn the_manifest_example_parses_as_a_manifest() {
    let blocks = fenced_blocks("toml");
    assert!(
        !blocks.is_empty(),
        "the readme has no toml block, so its manifest example is gone"
    );

    // Every block, rather than the first. A second example added later is a
    // second thing that can stop parsing, and a test reading one block reports
    // on one block whatever the readme grew.
    //
    // The one naming a workspace is the manifest. The rest are fragments of a
    // manifest and cannot stand alone, so each is folded into the manifest's
    // own keys and the result parsed, which is what checks that a fragment
    // names real keys carrying values of the right shape.
    let mut tables: Vec<toml::Table> = Vec::new();
    for block in &blocks {
        let parsed: Result<toml::Table, _> = toml::from_str(block);
        tables.push(parsed.unwrap_or_else(|e| {
            panic!("a toml block in the readme is not valid toml: {e}\n\n{block}")
        }));
    }
    let whole = tables
        .iter()
        .position(|t| t.contains_key("workspace"))
        .expect("no toml block in the readme names a workspace");

    for (i, fragment) in tables.iter().enumerate() {
        if i == whole {
            continue;
        }
        let mut merged = tables[whole].clone();
        for (k, v) in fragment {
            merged.insert(k.clone(), v.clone());
        }
        let parsed: Result<Config, _> = merged.clone().try_into();
        parsed.unwrap_or_else(|e| {
            panic!(
                "a toml fragment in the readme is not a manifest key: {e}\n\n{}",
                blocks[i]
            )
        });
    }

    let parsed: Result<Config, _> = tables[whole].clone().try_into();
    let config = parsed.unwrap_or_else(|e| {
        panic!(
            "the readme's manifest example does not parse: {e}\n\n{}",
            blocks[whole]
        )
    });

    // Parsing is not enough on its own: a config type that accepted anything
    // would pass the line above over an empty block. The example has to have
    // reached the fields it is there to show.
    assert_eq!(config.workspace.name, "my-stack");
    assert!(
        config.forges.contains_key("github"),
        "the example's forge did not survive the parse"
    );
    let repo = config
        .repos
        .get("notko")
        .expect("the example's repo did not survive the parse");
    assert_eq!(repo.forge, "github");
    assert_eq!(repo.local_path.as_os_str(), "notko");
}

/// One command the readme names, as it was written there.
struct Named {
    /// The words that form a command path: `["docs", "status"]`.
    path:         Vec<String>,
    /// Whether the readme wrote a `<placeholder>` after it.
    has_argument: bool,
    /// The whole span, for a message that points at what to fix.
    as_written:   String,
}

/// Every `` `homma ...` `` span in the readme, parsed.
fn commands_the_readme_names() -> Vec<Named> {
    let mut out: Vec<Named> = Vec::new();
    for line in README.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("`homma ") {
            let after = &rest[at + "`homma ".len() ..];
            let end = after.find('`').unwrap_or(after.len());
            let span = &after[.. end];
            rest = &after[end.min(after.len()) ..];

            let mut path = Vec::new();
            let mut has_argument = false;
            for word in span.split_whitespace() {
                if word.starts_with('<') || word.starts_with('[') {
                    has_argument = true;
                    break;
                }
                if word.starts_with('-') {
                    break;
                }
                path.push(word.to_string());
            }
            if path.is_empty()
                || out
                    .iter()
                    .any(|n| n.path == path && n.has_argument == has_argument)
            {
                continue;
            }
            out.push(Named {
                path,
                has_argument,
                as_written: format!("homma {span}"),
            });
        }
    }
    out
}

/// `--help` for a command path, as the shipped binary prints it.
fn help_for(path: &[String]) -> String {
    let out = bin()
        .args(path)
        .arg("--help")
        .output()
        .expect("the binary would not run");
    assert!(
        out.status.success(),
        "`homma {} --help` failed, so the readme names a command the cli does \
         not take:\n{}",
        path.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Whether clap says this command needs a subcommand to run.
///
/// Its usage line carries `<COMMAND>` when it does. Read off the shipped
/// binary rather than off the type, because the binary is what a reader who
/// copies a line out of the readme is going to run, and `homma-engine` has no
/// library target to reach the type through.
fn wants_a_subcommand(help: &str) -> bool {
    help.lines()
        .find(|l| l.starts_with("Usage:"))
        .is_some_and(|l| l.contains("<COMMAND>"))
}

/// **Every command the readme names runs as it is written there.**
///
/// The previous version ran each with `--help` and asked only whether that
/// succeeded. `--help` succeeds on a parent command whether or not the command
/// runs on its own, so the table offered `homma docs` for as long as it did,
/// and `homma docs` is a usage error.
#[test]
fn every_command_the_readme_names_runs_as_written() {
    let named = commands_the_readme_names();
    assert!(
        named.len() >= 8,
        "the readme names {} commands, too few to be the table this test is \
         about",
        named.len()
    );

    let mut wrong = Vec::new();
    for n in &named {
        let help = help_for(&n.path);
        match (wants_a_subcommand(&help), n.has_argument) {
            (true, false) => {
                wrong.push(format!(
                    "`{}` needs a subcommand and the readme writes it without one",
                    n.as_written
                ));
            },
            (false, true) => {
                wrong.push(format!(
                    "`{}` takes no subcommand and the readme writes one after it",
                    n.as_written
                ));
            },
            _ => {},
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The rows of the readme's command table.
///
/// The table specifically, rather than the whole readme. A command mentioned in
/// a paragraph further down is not documented as a command, and `agent` was
/// exactly that: a top-level command, discussed in the prose, and absent from
/// the table nobody would read past.
fn the_command_table() -> String {
    let after = README
        .split("| Command | What it's for |")
        .nth(1)
        .expect("the readme has no command table");
    after
        .split("\n\n")
        .next()
        .expect("the command table does not end")
        .to_string()
}

/// **Every command the cli has is in the readme's table.**
///
/// The check ran one way only, so a command added later is documented or this
/// says which one is not.
#[test]
fn every_command_the_cli_has_is_one_the_readme_names() {
    let help = help_for(&[]);
    let commands = help
        .split("\nCommands:\n")
        .nth(1)
        .expect("the top-level help has no command list");
    let commands = commands.split("\n\n").next().unwrap_or(commands);

    let table = the_command_table();
    // The parse of the table itself, so an empty one cannot report every
    // command as documented by matching nothing against nothing.
    assert!(
        table.lines().filter(|l| l.starts_with("| `homma ")).count() >= 8,
        "read too few rows out of the readme's command table:\n{table}"
    );

    let mut missing = Vec::new();
    let mut found = 0;
    for line in commands.lines() {
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        // clap's own, and not homma's to document.
        if name == "help" {
            continue;
        }
        found += 1;
        if !table.contains(&format!("`homma {name}")) {
            missing.push(name.to_string());
        }
    }
    // The parse, asserted rather than trusted: a help format this stopped
    // recognising would leave the loop above finding nothing and reporting
    // success.
    assert!(
        found >= 8,
        "read {found} commands out of the top-level help, which is too few to \
         be its command list"
    );
    assert!(
        missing.is_empty(),
        "the cli has commands the readme's table does not name: {missing:?}"
    );
}

/// **The readme names `token_cmd`.**
///
/// A manifest may carry an argument list homma runs to obtain a credential, so
/// a manifest taken from somewhere else runs a program of its choosing. The
/// readme said tokens come out of the environment and stopped there, which is
/// true and is the half that is safe.
#[test]
fn the_readme_names_the_credential_command_a_manifest_can_carry() {
    assert!(
        README.contains("token_cmd"),
        "the manifest can name a program to run for a credential and the readme \
         does not say so"
    );
}

#[test]
fn the_readme_names_the_output_flag_the_cli_actually_has() {
    // It used to say `--json`, which the cli has never taken.
    assert!(
        !README.contains("`--json`"),
        "the readme offers `--json`; the flag is `--output json`"
    );
    assert!(
        README.contains("`--output json`"),
        "the readme stopped naming the flag that makes its json claim true"
    );
    // A bad value for the flag is rejected by name, which proves the flag is
    // read without needing a workspace on disk to run `status` against.
    bin()
        .args(["--output", "json", "status", "--help"])
        .assert()
        .success();
}
