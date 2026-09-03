//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The description step: every published manifest's description against the
//! tagline of the readme it sits beside, the two lines a stranger reads first
//! on the registry and on the forge, held to being one line. Runs no program.

use std::path::{Path, PathBuf};

use homma_api::{Step, StepOutcome};

/// The readme's tagline: the blockquote under the title, which is the first
/// line beginning `>` after the first heading, wherever the badge block puts
/// it and inside a `<div>` or not. The marker comes off, and a quote wrapped
/// over several lines is joined by a space. `None` where the readme has no
/// heading, or no blockquote after it.
///
/// A title is an atx heading, `# name`, or a setext one, a line underlined
/// with `=` or `-`. The opening prose after the badge block is a different
/// element of the readme and is never taken for the tagline.
pub fn tagline(readme: &str) -> Option<String> {
    let lines: Vec<&str> = readme.lines().collect();
    let title = lines.iter().enumerate().find_map(|(i, line)| {
        let t = line.trim();
        if t.starts_with('#') {
            return Some(i);
        }
        let under = lines.get(i + 1).map(|u| u.trim()).unwrap_or("");
        let setext = !t.is_empty()
            && !t.starts_with('<')
            && !t.starts_with('>')
            && under.len() >= 3
            && (under.chars().all(|c| c == '=') || under.chars().all(|c| c == '-'));
        setext.then_some(i + 1)
    })?;
    let mut quote: Vec<&str> = Vec::new();
    for line in &lines[title + 1 ..] {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('>') {
            quote.push(rest.trim());
            continue;
        }
        if quote.is_empty() {
            continue;
        }
        // a lazy continuation: prose right under a `>` line is still the quote,
        // and anything else ends it
        if t.is_empty() || t.starts_with('<') || t.starts_with('#') {
            break;
        }
        quote.push(t);
    }
    let text = quote.join(" ").trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// One manifest that reaches a registry: where it is, what it declares, and
/// the readme it is held to.
#[derive(Debug, PartialEq, Eq)]
pub struct Published {
    /// Relative to the root, `launcher/Cargo.toml` or `deno.json`.
    pub manifest:    String,
    /// `None` where the manifest declares no description.
    pub description: Option<String>,
    /// Relative to the root: the readme beside the manifest where one exists,
    /// the root readme otherwise, and `None` where neither is there.
    pub readme:      Option<String>,
}

/// Every manifest in the tree that reaches a registry: a `Cargo.toml` whose
/// `[package]` does not say `publish = false`, and a `deno.json` with a
/// `name`. `target/`, `node_modules/` and `.git/` are not walked. A manifest
/// that does not parse is an error naming it.
pub fn published(root: &Path) -> Result<Vec<Published>, String> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    let relative = |p: &Path| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    while let Some(dir) = dirs.pop() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !matches!(name, "target" | "node_modules" | ".git") {
                    dirs.push(path);
                }
                continue;
            }
            let description = match name {
                "Cargo.toml" => cargo_description(&path)?,
                "deno.json" => deno_description(&path)?,
                _ => None,
            };
            let Some(description) = description else {
                continue;
            };
            let beside = dir.join("README.md");
            let readme = if beside.is_file() {
                Some(relative(&beside))
            } else if root.join("README.md").is_file() {
                Some("README.md".to_string())
            } else {
                None
            };
            found.push(Published {
                manifest: relative(&path),
                description,
                readme,
            });
        }
    }
    found.sort_by(|a, b| a.manifest.cmp(&b.manifest));
    Ok(found)
}

/// A `Cargo.toml`'s description where it names a published package: `None`
/// for a virtual root, a package saying `publish = false`, or no `[package]`
/// at all; `Some(None)` for a published package declaring no description.
fn cargo_description(path: &Path) -> Result<Option<Option<String>>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: toml::Value = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(package) = doc.get("package") else {
        return Ok(None);
    };
    if package.get("publish").and_then(|p| p.as_bool()) == Some(false) {
        return Ok(None);
    }
    Ok(Some(
        package
            .get("description")
            .and_then(|d| d.as_str())
            .map(str::to_string),
    ))
}

/// A `deno.json`'s description where it names a package, which is where it
/// carries a `name`; `None` for a plain config, `Some(None)` for a package
/// declaring no description.
fn deno_description(path: &Path) -> Result<Option<Option<String>>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if doc.get("name").and_then(|n| n.as_str()).is_none() {
        return Ok(None);
    }
    Ok(Some(
        doc.get("description")
            .and_then(|d| d.as_str())
            .map(str::to_string),
    ))
}

/// The step: each published manifest against the tagline of its readme,
/// trimmed at the ends and nothing else. A manifest without a description
/// fails like a wrong one; a readme without a tagline is a skip that says so;
/// a tree with no published manifest is a plain skip. The log names the
/// manifest of every pair and both strings where they differ.
pub fn check(root: &Path) -> StepOutcome {
    let mut outcome = StepOutcome {
        step:    Step::Description,
        passed:  true,
        skipped: false,
        numbers: Default::default(),
        log:     String::new(),
    };
    let manifests = match published(root) {
        Ok(m) => m,
        Err(e) => {
            outcome.passed = false;
            outcome.log.push_str(&format!("{e}\n"));
            return outcome;
        },
    };
    if manifests.is_empty() {
        return StepOutcome::skipped(Step::Description);
    }
    let mut compared = 0usize;
    for m in manifests {
        let tag = m
            .readme
            .as_ref()
            .and_then(|r| std::fs::read_to_string(root.join(r)).ok())
            .and_then(|text| tagline(&text));
        let Some(tag) = tag else {
            outcome.log.push_str(&format!(
                "{}: no tagline in {} to compare against; skipped\n",
                m.manifest,
                m.readme
                    .as_deref()
                    .unwrap_or("a readme, of which there is none")
            ));
            continue;
        };
        compared += 1;
        match m.description.as_deref().map(str::trim) {
            None => {
                outcome.passed = false;
                outcome.log.push_str(&format!(
                    "{}: declares no description; the tagline is:\n  {tag}\n",
                    m.manifest
                ));
            },
            Some(d) if d == tag => {
                outcome
                    .log
                    .push_str(&format!("{}: the description is the tagline\n", m.manifest));
            },
            Some(d) => {
                outcome.passed = false;
                outcome.log.push_str(&format!(
                    "{}: the description is not the readme's tagline\n  description: {d}\n  tagline:     {tag}\n",
                    m.manifest
                ));
            },
        }
    }
    // every manifest measured against nothing is a skip, not a pass
    outcome.skipped = compared == 0;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `notko/README.md`, the first thirteen lines, verbatim, and an opening
    /// paragraph after the badge block of the kind every readme here has.
    const NOTKO: &str = "# `notko`\n\n<div align=\"center\" style=\"text-align: center;\">\n\n[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/notko.svg)](https://github.com/orgrinrt/notko/stargazers)\n[![Crates.io](https://img.shields.io/crates/v/notko)](https://crates.io/crates/notko)\n[![docs.rs](https://img.shields.io/docsrs/notko)](https://docs.rs/notko)\n[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/notko.svg)](https://github.com/orgrinrt/notko/issues)\n![License](https://img.shields.io/github/license/orgrinrt/notko?color=%23009689)\n\n> Fallibility primitives for `no_std` rust. Ships `Just`, `Maybe` and `Outcome`, and a `#[profile]` attribute for tagging whole functions.\n\n</div>\n\nSome opening prose that is not the tagline.\n\n## What\n";
    /// `homma/README.md`, the first thirteen lines, verbatim.
    const HOMMA: &str = "# `homma`\n\n<div align=\"center\" style=\"text-align: center;\">\n\n[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/stargazers)\n[![Crates.io](https://img.shields.io/crates/v/homma)](https://crates.io/crates/homma)\n[![docs.rs](https://img.shields.io/docsrs/homma)](https://docs.rs/homma)\n[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/homma.svg)](https://github.com/orgrinrt/homma/issues)\n![License](https://img.shields.io/github/license/orgrinrt/homma?color=%23009689)\n\n> One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli.\n\n</div>\n";
    /// `beech/README.md`, the first fourteen lines, verbatim: a setext title,
    /// with a later section of the kind that was once returned instead.
    const BEECH: &str = "beech\n============\n\n<div style=\"text-align: center;\">\n\n[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/beech.svg)](https://github.com/orgrinrt/beech/stargazers)\n[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/beech.svg)](https://github.com/orgrinrt/beech/issues)\n[![Latest Version](https://img.shields.io/badge/version-0.0.1-red.svg?label=latest)](https://github.com/orgrinrt/beech)\n![GitHub last commit](https://img.shields.io/github/last-commit/orgrinrt/beech?color=%23009689&link=https%3A%2F%2Fgithub.com%2Forgrinrt%2Fbeech)\n\n> Write procedural macros with less boilerplate.\n\n</div>\n\n## Support\n\nBuy the author a coffee.\n";

    #[test]
    fn the_tagline_is_the_blockquote_under_the_title_inside_the_badge_block() {
        assert_eq!(
            tagline(NOTKO).as_deref(),
            Some(
                "Fallibility primitives for `no_std` rust. Ships `Just`, `Maybe` and `Outcome`, and a `#[profile]` attribute for tagging whole functions."
            )
        );
        assert_eq!(
            tagline(HOMMA).as_deref(),
            Some(
                "One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli."
            )
        );
        // a setext title is a title, and the support section is never reached
        assert_eq!(
            tagline(BEECH).as_deref(),
            Some("Write procedural macros with less boilerplate.")
        );
    }

    #[test]
    fn the_marker_comes_off_and_a_wrapped_quote_is_one_line() {
        assert_eq!(
            tagline("# x\n\n> The tagline, said once.\n\nProse.\n").as_deref(),
            Some("The tagline, said once.")
        );
        assert_eq!(
            tagline("# x\n>No space after the marker.\n").as_deref(),
            Some("No space after the marker.")
        );
        assert_eq!(
            tagline("# x\n\n> The tagline,\n> over two lines.\n\nProse.\n").as_deref(),
            Some("The tagline, over two lines.")
        );
        // a lazy continuation belongs to the quote, and a blank line ends it
        assert_eq!(
            tagline("# x\n\n> The tagline,\nlazily continued.\n\n> Not this one.\n").as_deref(),
            Some("The tagline, lazily continued.")
        );
        // the badges before it and a closing tag after it are not part of it
        assert_eq!(
            tagline("# x\n\n[![a](x)](y)\n> Quoted.\n</div>\n\nProse.\n").as_deref(),
            Some("Quoted.")
        );
    }

    #[test]
    fn no_heading_or_no_blockquote_after_it_is_no_tagline() {
        assert_eq!(tagline(""), None);
        assert_eq!(tagline("> a quote with no title above it\n"), None);
        assert_eq!(tagline("# x\n"), None);
        // the opening prose is not the tagline, however it is laid out
        assert_eq!(tagline("# x\n\nOpening prose.\n\n## Next\n"), None);
        assert_eq!(
            tagline("# x\n\n<div>\n[![a](x)](y)\n</div>\n\nOpening prose.\n"),
            None
        );
        // an underline shorter than three is not a setext title
        assert_eq!(tagline("x\n==\n\n> quoted\n"), None);
        // an empty quote is no tagline
        assert_eq!(tagline("# x\n\n>\n\nProse.\n"), None);
    }

    fn root(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (name, body) in files {
            let path = d.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        d
    }

    const README: &str = "# x\n\n> One line, said once.\n\nMore.\n";
    const CRATE: &str =
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\ndescription = \"One line, said once.\"\n";

    #[test]
    fn the_published_manifests_are_the_packages_that_reach_a_registry() {
        let d = root(&[
            ("README.md", README),
            // a virtual root is not a package
            ("Cargo.toml", "[workspace]\nmembers = [\"launcher\"]\n"),
            ("launcher/Cargo.toml", CRATE),
            // an unpublished crate is nobody's registry line
            (
                "mock/crates/engine/Cargo.toml",
                "[package]\nname = \"engine\"\nversion = \"0.1.0\"\npublish = false\ndescription = \"x\"\n",
            ),
            // a member with a readme of its own is held to that one
            ("member/Cargo.toml", CRATE),
            ("member/README.md", README),
            // a plain deno config is not a package; a named one is
            ("deno.json", "{\"tasks\": {}}"),
            (
                "pkg/deno.json",
                "{\"name\": \"@h/x\", \"description\": \"One line, said once.\"}",
            ),
            // never walked
            ("target/debug/Cargo.toml", CRATE),
            ("node_modules/x/deno.json", "{\"name\": \"x\"}"),
        ]);
        assert_eq!(published(d.path()).unwrap(), vec![
            Published {
                manifest:    "launcher/Cargo.toml".into(),
                description: Some("One line, said once.".into()),
                readme:      Some("README.md".into()),
            },
            Published {
                manifest:    "member/Cargo.toml".into(),
                description: Some("One line, said once.".into()),
                readme:      Some("member/README.md".into()),
            },
            Published {
                manifest:    "pkg/deno.json".into(),
                description: Some("One line, said once.".into()),
                readme:      Some("README.md".into()),
            },
        ]);
        // no readme anywhere is `None`, and a package declaring no description is too
        let d = root(&[(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )]);
        assert_eq!(published(d.path()).unwrap(), vec![Published {
            manifest:    "Cargo.toml".into(),
            description: None,
            readme:      None,
        }]);
        // a manifest that does not parse is an error naming it
        let d = root(&[("Cargo.toml", "[package\n")]);
        assert!(published(d.path()).unwrap_err().contains("Cargo.toml"));
    }

    #[test]
    fn the_description_that_is_the_tagline_passes_and_a_different_one_fails_naming_both() {
        let d = root(&[("README.md", README), ("Cargo.toml", CRATE)]);
        let out = check(d.path());
        assert!(out.passed && !out.skipped, "{}", out.log);
        assert!(
            out.log
                .contains("Cargo.toml: the description is the tagline")
        );
        let d = root(&[
            ("README.md", README),
            (
                "Cargo.toml",
                "[package]\nname = \"x\"\nversion = \"0.1.0\"\ndescription = \"One line, said once\"\n",
            ),
        ]);
        let out = check(d.path());
        assert!(
            !out.passed,
            "a trailing full stop is the drift being caught"
        );
        assert!(
            out.log.contains("Cargo.toml: the description is not"),
            "{}",
            out.log
        );
        assert!(
            out.log.contains("description: One line, said once\n"),
            "{}",
            out.log
        );
        assert!(
            out.log.contains("tagline:     One line, said once.\n"),
            "{}",
            out.log
        );
        // whitespace at the ends is not a difference
        let d = root(&[
            ("README.md", README),
            (
                "Cargo.toml",
                "[package]\nname = \"x\"\nversion = \"0.1.0\"\ndescription = \"  One line, said once.  \"\n",
            ),
        ]);
        assert!(check(d.path()).passed);
    }

    #[test]
    fn a_launcher_under_a_virtual_root_is_held_to_the_root_readme_and_the_engine_is_not_read() {
        let d = root(&[
            ("README.md", HOMMA),
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"launcher\"]\nexclude = [\"mock\"]\n",
            ),
            (
                "launcher/Cargo.toml",
                "[package]\nname = \"homma\"\nversion = \"0.1.0\"\ndescription = \"One command for a directory of repositories that belong together. Speaks git and the forge apis itself, no shelling out to a provider cli.\"\n",
            ),
            (
                "mock/crates/homma-core/Cargo.toml",
                "[package]\nname = \"homma-core\"\nversion = \"0.1.0\"\npublish = false\ndescription = \"Core library for homma\"\n",
            ),
        ]);
        let out = check(d.path());
        assert!(out.passed && !out.skipped, "{}", out.log);
        assert!(
            out.log
                .contains("launcher/Cargo.toml: the description is the tagline")
        );
        assert!(!out.log.contains("homma-core"), "{}", out.log);
    }

    #[test]
    fn every_published_manifest_is_held_and_one_wrong_fails_the_step() {
        let d = root(&[
            ("README.md", README),
            ("Cargo.toml", CRATE),
            (
                "deno.json",
                "{\"name\": \"@h/x\", \"description\": \"Another line.\"}",
            ),
        ]);
        let out = check(d.path());
        assert!(!out.passed, "both manifests are held to it");
        assert!(
            out.log.contains("deno.json: the description is not"),
            "{}",
            out.log
        );
        assert!(
            out.log
                .contains("Cargo.toml: the description is the tagline"),
            "{}",
            out.log
        );
    }

    #[test]
    fn a_missing_description_fails_and_a_missing_tagline_skips_and_says_so() {
        let d = root(&[
            ("README.md", README),
            (
                "Cargo.toml",
                "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
            ),
        ]);
        let out = check(d.path());
        assert!(!out.passed && !out.skipped);
        assert!(
            out.log.contains("Cargo.toml: declares no description"),
            "{}",
            out.log
        );
        // a readme with opening prose and no blockquote has no tagline
        let d = root(&[("README.md", "# x\n\nOpening prose only.\n"), ("Cargo.toml", CRATE)]);
        let out = check(d.path());
        assert!(out.skipped && out.passed, "{}", out.log);
        assert!(
            out.log.contains("Cargo.toml: no tagline in README.md"),
            "{}",
            out.log
        );
        // no readme at all is the same skip
        let d = root(&[("Cargo.toml", CRATE)]);
        let out = check(d.path());
        assert!(out.skipped, "{}", out.log);
        assert!(out.log.contains("of which there is none"), "{}", out.log);
        // one manifest measured and one not is a measurement, not a skip
        let d = root(&[("Cargo.toml", CRATE), ("a/Cargo.toml", CRATE), ("a/README.md", README)]);
        let out = check(d.path());
        assert!(out.passed && !out.skipped, "{}", out.log);
        // a manifest that does not parse is a failure that says which
        let d = root(&[("README.md", README), ("Cargo.toml", "[package\n")]);
        let out = check(d.path());
        assert!(!out.passed);
        assert!(out.log.contains("Cargo.toml"), "{}", out.log);
    }

    #[test]
    fn a_tree_with_no_published_manifest_skips_the_step() {
        let d = root(&[("README.md", README)]);
        assert!(check(d.path()).skipped);
        let d = root(&[("README.md", README), ("Cargo.toml", "[workspace]\nmembers = []\n")]);
        assert!(check(d.path()).skipped);
    }
}
