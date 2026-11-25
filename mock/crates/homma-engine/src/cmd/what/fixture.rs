//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The small workspace the tests of `homma what` read: planted once per test
//! binary, with neutral names, so no test reads the tree the repository happens
//! to sit in.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The planted workspace's root.
pub(super) fn workspace() -> PathBuf {
    static ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
    ROOT.get_or_init(|| {
        let d = tempfile::tempdir().expect("a temporary directory");
        plant(d.path());
        d
    })
    .path()
    .to_path_buf()
}

fn file(root: &Path, rel: &str, text: &str, exec: bool) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    #[cfg(unix)]
    if exec {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = exec;
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
        "space-trim",
        "give the disk back: stale build output and spent work trees, dry run by default",
        &["hygiene", "disk"],
        &[
            ("trim", "print what would be freed; --apply frees it"),
            ("report", "what the last automatic run freed, once"),
        ],
    );
    tool(
        root,
        "stale-targets",
        "give back the disk space held by stale build targets nothing links",
        &["hygiene", "disk"],
        &[(
            "sweep",
            "remove the stale build targets; dry run by default",
        )],
    );
    tool(
        root,
        "lost-runs",
        "find the build and test processes a session lost track of, and end the ones named",
        &["hygiene", "processes"],
        &[
            (
                "list",
                "one line per build or test process found in the workspace",
            ),
            (
                "end",
                "end the named processes, refusing any outside the workspace",
            ),
        ],
    );
    file(
        root,
        ".shared/scripts/remote/land-branch",
        "#!/bin/sh\n# land-branch - land a branch onto the trunk once its gate is green\n",
        true,
    );
    file(
        root,
        ".shared/scripts/remote/sessions",
        "#!/bin/sh\n# ====\n# sessions - which helper sessions are finished or stuck\n# ====\n#   sessions finished <arc>   ids of idle helpers whose channel says done\n#   sessions blocked <arc>    ids of helpers stopped on a question\n",
        true,
    );
    file(
        root,
        ".shared/scripts/remote/landing-paths.sh",
        "#!/bin/sh\n# Sourced by the landing scripts: where a branch lands and how it is named.\n",
        true,
    );
    file(
        root,
        ".shared/scripts/remote/land-branch_test.sh",
        "#!/bin/sh\n# The tests of land-branch.\n",
        true,
    );
    file(
        root,
        ".shared/scripts/remote/land-branch_fixture.sh",
        "#!/bin/sh\n# A trunk for the tests of land-branch.\n",
        true,
    );
    file(
        root,
        ".shared/skills/work-in-rounds/SKILL.md.tmpl",
        "---\nname: work-in-rounds\ndescription: Use when working a design round: topic, changelists, locks and close.\n---\n\n# work-in-rounds\n\nThe flow of a round.\n",
        false,
    );
    file(
        root,
        ".shared/rules/tests-first.md.tmpl",
        "---\ntopics: [tests, controls]\nfires: \"before writing a test\"\nkind: reflex\n---\n\n# tests-first\n\nEach test is shown failing first.\n",
        false,
    );
}
