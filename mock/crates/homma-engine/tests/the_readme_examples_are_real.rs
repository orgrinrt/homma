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
    assert_eq!(
        blocks.len(),
        1,
        "the readme has {} toml blocks; this test knows about one and would \
         silently skip the rest",
        blocks.len()
    );

    let parsed: Result<Config, _> = toml::from_str(&blocks[0]);
    let config = parsed.unwrap_or_else(|e| {
        panic!(
            "the readme's manifest example does not parse: {e}\n\n{}",
            blocks[0]
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

#[test]
fn every_command_the_readme_names_is_one_the_cli_takes() {
    // The readme's table spells each one as `homma <cmd>`, so that is what is
    // looked for.
    let mut named: Vec<&str> = Vec::new();
    for line in README.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("`homma ") {
            let after = &rest[at + "`homma ".len() ..];
            let end = after.find('`').unwrap_or(after.len());
            let cmd = after[.. end].split_whitespace().next().unwrap_or("");
            if !cmd.is_empty() && !cmd.starts_with('-') && !named.contains(&cmd) {
                named.push(cmd);
            }
            rest = &after[end.min(after.len()) ..];
        }
    }

    assert!(
        named.len() >= 8,
        "the readme names {} commands, too few to be the table this test is \
         about: {named:?}",
        named.len()
    );

    for cmd in &named {
        // `--help` parses the subcommand and then stops, so a zero exit means
        // the name was recognised and nothing was run.
        bin().args([cmd, "--help"]).assert().success();
    }
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
