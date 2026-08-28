//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A test named in `include` runs where the package lands, or it is not named.
//!
//! `cargo publish` cannot tell you this. Verification builds the lib and the
//! bins and no test at all, so a test that only works from a checkout passes
//! every gate on the way out and fails for whoever unpacks the crate, against
//! a version that cannot be replaced.
//!
//! Two routes out of the package, and this repository has produced both.
//!
//! **A stripped dependency.** Cargo drops a dev-dependency carrying a path and
//! no version, because there is nothing for the registry to resolve. The
//! import then names a crate the tarball does not have.
//!
//! **A path above the package root.** `CARGO_MANIFEST_DIR` is the package in
//! the tarball and the crate directory in a checkout, and everything above it
//! is the difference between the two. A test walking to `.parent()`, or an
//! `include_str!` climbing with `../`, reads a repository that is not there.
//!
//! An earlier version of this file checked the first route only, while its own
//! name promised both, and the four tests that shipped alongside it all took
//! the second. That is why the check is written against the manifest rather
//! than against a list of known-bad imports: the manifest is the thing that
//! decides what ships.

use std::collections::BTreeSet;
use std::path::Path;

/// The dependencies cargo strips: named by path, carrying no version, under
/// `[dev-dependencies]`.
///
/// A regular or build dependency of that shape is refused outright at publish
/// rather than stripped, so it is a different defect and not this one's.
fn stripped_dev_deps(manifest: &str) -> BTreeSet<String> {
    let Some((_, rest)) = manifest.split_once("[dev-dependencies]") else {
        return BTreeSet::new();
    };
    rest.split("\n[")
        .next()
        .unwrap_or(rest)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, spec) = line.split_once('=')?;
            (spec.contains("path") && !spec.contains("version"))
                .then(|| name.trim().replace('-', "_"))
        })
        .collect()
}

/// The test files an `include` list names, as written.
fn shipped_tests(manifest: &str) -> BTreeSet<String> {
    let Some((_, rest)) = manifest.split_once("\ninclude") else {
        // No `include` at all ships every test in the crate, which is a
        // different and looser state than naming none.
        return BTreeSet::new();
    };
    let Some((body, _)) = rest.split_once(']') else {
        return BTreeSet::new();
    };
    body.split(['"', '\''])
        .filter(|s| s.starts_with("tests/") && s.ends_with(".rs"))
        .map(str::to_owned)
        .collect()
}

/// Why a given test cannot run in the tarball, if it cannot.
fn cannot_run_in_the_tarball(body: &str, stripped: &BTreeSet<String>) -> Vec<String> {
    let mut why = Vec::new();

    for dep in stripped {
        if body.contains(&format!("use {dep}")) || body.contains(&format!("{dep}::")) {
            why.push(format!("names `{dep}`, which cargo strips on publish"));
        }
    }

    // `CARGO_MANIFEST_DIR` is the package root in a tarball, so walking above
    // it reaches the repository and nothing else.
    if body.contains("CARGO_MANIFEST_DIR") && body.contains(".parent()") {
        why.push(
            "walks above `CARGO_MANIFEST_DIR`, which in a tarball is the \
             package root and has nothing above it"
                .to_string(),
        );
    }
    for macro_name in ["include_str!", "include_bytes!", "include!"] {
        let mut rest = body;
        while let Some(at) = rest.find(macro_name) {
            let after = &rest[at + macro_name.len() ..];
            let Some(open) = after.find('"') else { break };
            let tail = &after[open + 1 ..];
            let Some(close) = tail.find('"') else { break };
            let path = &tail[.. close];
            // One `../` climbs out of `tests/` to the package root, which is
            // fine. A second leaves the package.
            if path.matches("../").count() > 1 {
                why.push(format!(
                    "includes `{path}`, which climbs out of the package"
                ));
            }
            rest = &tail[close ..];
        }
    }

    why
}

#[test]
fn include_names_no_test_that_cannot_run_where_the_package_lands() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("no manifest");

    let stripped = stripped_dev_deps(&manifest);
    let shipped = shipped_tests(&manifest);

    // Naming none is the current state and a legitimate one: every test here
    // is a test about the repository. It is asserted rather than passed over,
    // because the alternative reading of an empty set is that the parse broke.
    if shipped.is_empty() {
        assert!(
            manifest.contains("\ninclude"),
            "the manifest has no `include`, so every test in this crate ships \
             and this check has no list to work from"
        );
        return;
    }

    let mut problems = Vec::new();
    for test in &shipped {
        let body = match std::fs::read_to_string(dir.join(test)) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!(
                    "{test} is named in `include` and cannot be read: {e}"
                ));
                continue;
            },
        };
        for why in cannot_run_in_the_tarball(&body, &stripped) {
            problems.push(format!("{test} {why}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_check_catches_both_routes_out_of_the_package() {
    // The control, and it is the four tests this repository actually shipped,
    // reduced to the line that made each of them unrunnable.
    let stripped: BTreeSet<String> = ["homma_core".to_string()].into_iter().collect();

    let walks_up = "let root = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).parent().unwrap();";
    assert_eq!(cannot_run_in_the_tarball(walks_up, &stripped).len(), 1);

    let stripped_import = "use homma_core::config::Config;";
    assert_eq!(
        cannot_run_in_the_tarball(stripped_import, &stripped).len(),
        1
    );

    let escapes = r#"const R: &str = include_str!("../../README.md");"#;
    assert_eq!(cannot_run_in_the_tarball(escapes, &stripped).len(), 1);

    // And the shapes that are fine, so the check is not simply always positive.
    let own_readme = r#"const R: &str = include_str!("../README.md");"#;
    assert!(cannot_run_in_the_tarball(own_readme, &stripped).is_empty());

    let own_binary = "let exe = env!(\"CARGO_BIN_EXE_homma\");";
    assert!(cannot_run_in_the_tarball(own_binary, &stripped).is_empty());

    // The parse, over the manifest shapes it has to read.
    assert!(shipped_tests("include = [\"src/**/*.rs\"]\n").is_empty());
    assert_eq!(
        shipped_tests("\ninclude = [\"src/**/*.rs\", \"tests/a.rs\"]\n").len(),
        1
    );
    let deps = "[dev-dependencies]\nhomma-core = { path = \"../x\" }\ntempfile = \"3\"\n";
    let s = stripped_dev_deps(deps);
    assert!(s.contains("homma_core"));
    assert!(
        !s.contains("tempfile"),
        "a registry dev-dep is not stripped"
    );
}
