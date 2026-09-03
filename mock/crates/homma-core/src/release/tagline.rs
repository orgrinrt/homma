//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The description step: the manifest's description against the readme's
//! tagline, the two lines a stranger reads first on the registry and on the
//! forge, held to being one line. Runs no program; reads two files.

use std::path::Path;

use homma_api::{RepoKind, Step, StepOutcome};

/// The readme's tagline: the first paragraph of prose after the title, past
/// the badge block and any blank line. `None` where the readme has no title
/// or nothing after it that reads as prose.
///
/// A badge block is html, a `<div>` and what it holds, or lines that are
/// nothing but images and links; a paragraph is the lines up to the next
/// blank one, joined by a space, since a tagline written over two lines is
/// still one sentence.
pub fn tagline(readme: &str) -> Option<String> {
    let mut lines = readme.lines();
    // the title: the first `#` heading, wherever the badges sit
    let mut titled = false;
    for line in lines.by_ref() {
        if line.trim_start().starts_with('#') {
            titled = true;
            break;
        }
    }
    if !titled {
        return None;
    }
    let mut in_html = 0usize;
    let mut paragraph: Vec<&str> = Vec::new();
    for line in lines {
        let t = line.trim();
        if in_html > 0 {
            in_html += t.matches("<div").count();
            in_html = in_html.saturating_sub(t.matches("</div>").count());
            continue;
        }
        if t.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if t.starts_with("<div") {
            in_html = t.matches("<div").count();
            in_html = in_html.saturating_sub(t.matches("</div>").count());
            continue;
        }
        // a heading before any prose is a readme with no tagline: the first
        // paragraph is the one under the title, not the one under a section
        if t.starts_with('#') {
            break;
        }
        if t.starts_with('<') || is_badges(t) {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        paragraph.push(t);
    }
    if paragraph.is_empty() { None } else { Some(paragraph.join(" ")) }
}

/// A line that is images and links and nothing else, which is a badge row
/// however many words its urls carry.
fn is_badges(line: &str) -> bool {
    let mut rest = line.trim();
    let mut any = false;
    while !rest.is_empty() {
        if !(rest.starts_with("[![") || rest.starts_with("![") || rest.starts_with('[')) {
            return false;
        }
        // the end of the outermost link: the `)` that closes its `(`, with a
        // badge's image link nested inside it
        let mut depth = 0i32;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && !rest[i + 1 ..].starts_with("](") {
                        end = Some(i + 1);
                        break;
                    }
                },
                _ => {},
            }
        }
        match end {
            Some(e) => {
                rest = rest[e ..].trim_start();
                any = true;
            },
            None => return false,
        }
    }
    any
}

/// The description a `Cargo.toml` declares: under `[package]`, or under
/// `[workspace.package]` where the root is virtual. `Ok(None)` where the
/// manifest declares none.
pub fn cargo_description(root: &Path) -> Result<Option<String>, String> {
    let text =
        std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let doc: toml::Value = toml::from_str(&text).map_err(|e| format!("Cargo.toml: {e}"))?;
    let at = |keys: &[&str]| {
        let mut v = &doc;
        for k in keys {
            v = v.get(k)?;
        }
        v.as_str().map(str::to_string)
    };
    Ok(at(&["package", "description"]).or_else(|| at(&["workspace", "package", "description"])))
}

/// The description a `deno.json` declares, `Ok(None)` where it declares none.
pub fn deno_description(root: &Path) -> Result<Option<String>, String> {
    let text =
        std::fs::read_to_string(root.join("deno.json")).map_err(|e| format!("deno.json: {e}"))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("deno.json: {e}"))?;
    Ok(doc
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// One manifest's description, or why it could not be read.
type Reader = fn(&Path) -> Result<Option<String>, String>;

/// The step: each manifest the kind names against the readme's tagline,
/// trimmed at the ends and nothing else. A readme without a tagline is a
/// skip that says so; a manifest without a description fails like a wrong
/// one; the log names both strings where they differ.
pub fn check(root: &Path, repo_kind: RepoKind) -> StepOutcome {
    let mut manifests: Vec<(&str, Reader)> = Vec::new();
    if repo_kind.has_crate() {
        manifests.push(("Cargo.toml", cargo_description));
    }
    if repo_kind.has_deno() {
        manifests.push(("deno.json", deno_description));
    }
    if manifests.is_empty() {
        return StepOutcome::skipped(Step::Description);
    }
    let mut outcome = StepOutcome {
        step:    Step::Description,
        passed:  true,
        skipped: false,
        numbers: Default::default(),
        log:     String::new(),
    };
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap_or_default();
    let Some(tag) = tagline(&readme) else {
        outcome.skipped = true;
        outcome
            .log
            .push_str("no tagline in README.md to compare against; skipped\n");
        return outcome;
    };
    let tag = tag.trim();
    for (name, read) in manifests {
        match read(root) {
            Err(e) => {
                outcome.passed = false;
                outcome.log.push_str(&format!("{e}\n"));
            },
            Ok(None) => {
                outcome.passed = false;
                outcome.log.push_str(&format!(
                    "{name} declares no description; the readme's tagline is:\n  {tag}\n"
                ));
            },
            Ok(Some(d)) if d.trim() == tag => {
                outcome
                    .log
                    .push_str(&format!("{name}: the description is the tagline\n"));
            },
            Ok(Some(d)) => {
                outcome.passed = false;
                outcome.log.push_str(&format!(
                    "{name}: the description is not the readme's tagline\n  description: {}\n  tagline:     {tag}\n",
                    d.trim()
                ));
            },
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tagline_is_the_first_prose_after_the_title_past_the_badges() {
        let plain = "# tyvi\n\nCore library for devspace orchestration.\n\nMore below.\n";
        assert_eq!(
            tagline(plain).as_deref(),
            Some("Core library for devspace orchestration.")
        );
        let badged = "# `notko`\n\n<div align=\"center\" style=\"text-align: center;\">\n\n[![a](x)](y) [![b](x)](y)\n\n</div>\n\nFallibility primitives whose branch cost is picked at the call site.\n\n## What\n";
        assert_eq!(
            tagline(badged).as_deref(),
            Some("Fallibility primitives whose branch cost is picked at the call site.")
        );
        let rows = "# x\n\n[![a](x)](y)\n![b](x)\n[docs](https://d)\n\nThe thing, said once.\n";
        assert_eq!(tagline(rows).as_deref(), Some("The thing, said once."));
        // a tagline wrapped over two lines is one line
        let wrapped = "# x\n\nThe thing,\nsaid over two lines.\n\nRest.\n";
        assert_eq!(
            tagline(wrapped).as_deref(),
            Some("The thing, said over two lines.")
        );
        // badges above the title are not the title
        let above = "[![a](x)](y)\n\n# x\n\nAfter the badges.\n";
        assert_eq!(tagline(above).as_deref(), Some("After the badges."));
    }

    #[test]
    fn no_title_or_no_prose_is_no_tagline() {
        assert_eq!(tagline(""), None);
        assert_eq!(tagline("just a line\n"), None);
        assert_eq!(tagline("# x\n"), None);
        assert_eq!(
            tagline("# x\n\n<div>\n[![a](x)](y)\n</div>\n\n## Next\n"),
            None
        );
        assert_eq!(
            tagline("# x\n\n## Straight to a section\n\nprose\n"),
            None,
            "a heading is not prose"
        );
    }

    #[test]
    fn a_badge_row_is_images_and_links_and_nothing_else() {
        assert!(is_badges("[![a](x)](y) [![b](x)](y)"));
        assert!(is_badges("![a](x)"));
        assert!(is_badges("[docs](https://d)"));
        assert!(!is_badges("[docs](https://d) and prose"));
        assert!(!is_badges("prose"));
        assert!(!is_badges("[unclosed"));
    }

    fn root(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (name, body) in files {
            std::fs::write(d.path().join(name), body).unwrap();
        }
        d
    }

    const README: &str = "# x\n\nOne line, said once.\n\nMore.\n";
    const CRATE: &str =
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\ndescription = \"One line, said once.\"\n";

    #[test]
    fn the_description_that_is_the_tagline_passes_and_a_different_one_fails_naming_both() {
        let d = root(&[("README.md", README), ("Cargo.toml", CRATE)]);
        let out = check(d.path(), RepoKind::Crate);
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
        let out = check(d.path(), RepoKind::Crate);
        assert!(
            !out.passed,
            "a trailing full stop is the drift being caught"
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
        assert!(check(d.path(), RepoKind::Crate).passed);
    }

    #[test]
    fn a_virtual_root_reads_the_workspace_package_and_a_deno_package_its_json() {
        let d = root(&[
            ("README.md", README),
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\"]\n[workspace.package]\ndescription = \"One line, said once.\"\n",
            ),
            (
                "deno.json",
                "{\"name\": \"@h/x\", \"description\": \"One line, said once.\"}",
            ),
        ]);
        assert!(check(d.path(), RepoKind::Both).passed);
        let d = root(&[
            ("README.md", README),
            ("Cargo.toml", CRATE),
            (
                "deno.json",
                "{\"name\": \"@h/x\", \"description\": \"Another line.\"}",
            ),
        ]);
        let out = check(d.path(), RepoKind::Both);
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
        let out = check(d.path(), RepoKind::Crate);
        assert!(!out.passed && !out.skipped);
        assert!(out.log.contains("declares no description"), "{}", out.log);
        let d = root(&[("README.md", "# x\n\n## Sections only\n"), ("Cargo.toml", CRATE)]);
        let out = check(d.path(), RepoKind::Crate);
        assert!(out.skipped && out.passed);
        assert!(out.log.contains("no tagline"), "{}", out.log);
        // no readme at all is the same skip
        let d = root(&[("Cargo.toml", CRATE)]);
        assert!(check(d.path(), RepoKind::Crate).skipped);
        // a manifest that does not parse is a failure that says which
        let d = root(&[("README.md", README), ("Cargo.toml", "[package\n")]);
        let out = check(d.path(), RepoKind::Crate);
        assert!(!out.passed);
        assert!(out.log.starts_with("Cargo.toml:"), "{}", out.log);
    }

    #[test]
    fn a_content_repository_skips_the_step() {
        let d = root(&[("README.md", README)]);
        assert!(check(d.path(), RepoKind::Content).skipped);
    }
}
