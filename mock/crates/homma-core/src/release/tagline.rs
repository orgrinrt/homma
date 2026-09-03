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
/// line beginning `>` between the first heading and the next heading after
/// it, wherever the badge block puts it and inside a `<div>` or not. Fenced
/// code is skipped, the marker comes off, and a quote wrapped over several
/// lines is joined by a space. `None` where the readme has no heading, or no
/// blockquote before its first section.
///
/// A title is an atx heading, `# name`, or a setext one, a line underlined
/// with `=` or `-`. Nothing below the first section is read, so the opening
/// prose is never taken for the tagline and neither is a quote a later
/// section carries.
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
    let mut fenced = false;
    for line in &lines[title + 1 ..] {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            if !quote.is_empty() {
                break;
            }
            continue;
        }
        if fenced {
            continue;
        }
        // the first section ends the search, quote in hand or not
        if t.starts_with('#') {
            break;
        }
        if let Some(rest) = t.strip_prefix('>') {
            quote.push(rest.trim());
            continue;
        }
        if quote.is_empty() {
            continue;
        }
        // a lazy continuation: prose right under a `>` line is still the quote,
        // and anything else ends it
        if t.is_empty() || t.starts_with('<') {
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

/// A directory the walk leaves alone: build output, vendored packages, git's
/// own, and the audit trail, whose committed crates are evidence rather than
/// anything a registry sees.
fn unwalked(name: &str) -> bool {
    matches!(
        name,
        "target" | "node_modules" | ".git" | "research" | "design_rounds"
    ) || name.ends_with("probes")
}

/// Every manifest in the tree that reaches a registry: a `Cargo.toml` whose
/// `[package]` does not say `publish = false`, and a `deno.json` with a
/// `name`, outside the directories `unwalked` names. A manifest that does not
/// parse is an error naming it.
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
                if !unwalked(name) {
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
#[path = "tagline_tests.rs"]
mod tests;
