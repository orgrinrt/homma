//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The small workspace the tests of `homma what` read: planted by each test
//! that wants it, with neutral names, so no test reads the tree the repository
//! happens to sit in.

use std::path::Path;

/// A planted workspace, removed when the holder drops it.
pub(super) fn workspace() -> tempfile::TempDir {
    let d = tempfile::tempdir().expect("a temporary directory");
    plant(d.path());
    d
}

fn file(root: &Path, rel: &str, text: &str, exec: bool) {
    let mode = if exec { 0o755 } else { 0o644 };
    homma_fixture::script(&root.join(rel), text, mode);
}

/// A tool with its commands, each a script under `commands/`.
fn tool(root: &Path, name: &str, summary: &str, tags: &[&str], commands: &[(&str, &str)]) {
    let tags = tags
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut toml = format!(
        "[tool]\nname = \"{name}\"\nsummary = \"{summary}\"\ntags = [{tags}]\nbackend = \"local\"\npromote = true\n\n[tool.source]\npath = {{ path = \"tools/{name}\" }}\n"
    );
    for (verb, what) in commands {
        toml.push_str(&format!(
            "\n[[tool.commands]]\nname = \"{verb}\"\nsummary = \"{what}\"\nrun = \"commands/{verb}\"\n"
        ));
        file(
            root,
            &format!("tools/{name}/commands/{verb}"),
            "#!/bin/sh\n",
            true,
        );
    }
    file(root, &format!("tools/{name}/tool.toml"), &toml, false);
}

fn plant(root: &Path) {
    tool(
        root,
        "plot-weed",
        "clear the beds of weeds and dead stalks, dry run by default",
        &["garden", "beds"],
        &[
            ("pull", "print what would be pulled; --apply pulls it"),
            ("tally", "what the last weeding pulled, once"),
        ],
    );
    tool(
        root,
        "plot-mulch",
        "cover the beds with mulch where weeds and dead stalks were cleared",
        &["garden", "beds"],
        &[("spread", "spread the mulch; dry run by default")],
    );
    tool(
        root,
        "pest-watch",
        "find the pests that got into a bed, and catch the ones named",
        &["garden", "pests"],
        &[
            ("list", "one line per pest seen in the beds"),
            (
                "catch",
                "catch the named pests, refusing any outside the fence",
            ),
        ],
    );
    file(
        root,
        ".shared/scripts/chores/water-plants",
        "#!/bin/sh\n# water-plants - water the plants in a bed once its soil is dry\n",
        true,
    );
    file(
        root,
        ".shared/scripts/chores/harvest",
        "#!/bin/sh\n# ====\n# harvest - which crops are ripe or rotting\n# ====\n#   harvest ripe <bed>     crops ready to pick\n#   harvest rotten <bed>   crops past saving\n",
        true,
    );
    file(
        root,
        ".shared/scripts/chores/bed-names.sh",
        "#!/bin/sh\n# Sourced by the chore scripts: what each bed is called and where it lies.\n",
        true,
    );
    file(
        root,
        ".shared/scripts/chores/water-plants_test.sh",
        "#!/bin/sh\n# The tests of water-plants.\n",
        true,
    );
    file(
        root,
        ".shared/scripts/chores/water-plants_fixture.sh",
        "#!/bin/sh\n# A dry bed for the tests of water-plants.\n",
        true,
    );
    file(
        root,
        ".shared/skills/plan-a-season/SKILL.md.tmpl",
        "---\nname: plan-a-season\ndescription: Use when planning a season: beds, seeds, rotations and frost.\n---\n\n# plan-a-season\n\nThe order of a season.\n",
        false,
    );
    file(
        root,
        ".shared/rules/seeds-first.md.tmpl",
        "---\ntopics: [seeds, sowing]\nfires: \"before sowing a bed\"\nkind: reflex\n---\n\n# seeds-first\n\nEach seed is checked before it is sown.\n",
        false,
    );
}
