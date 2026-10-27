//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The scripts under `.shared/scripts`, described by the comment block each opens with.
//!
//! The block is the descriptor and is read in place, so there is no second list to go stale. It is the
//! run of comment lines after the shebang, in whichever spelling the language gives it: `#` lines, a
//! docstring or a `/** */` block. The first line of prose is the summary, with a leading `<name> - ` or
//! `<name>: ` taken off. A later line that begins with the script's own name is a usage line, and its
//! second word, when it is a plain lowercase word, names a verb. A line `tags: a, b` adds tags, and
//! the directories the script sits in are its place. A script without a block has no descriptor and is
//! counted rather than guessed at.

use std::path::{Path, PathBuf};

use super::super::doc::{Doc, Gap, Gathered, Kind, Verb};

/// Where the scripts are, from the workspace root.
pub const DIR: &str = ".shared/scripts";

/// Directories whose contents are not tools.
const SKIPPED: &[&str] = &[".venv", "node_modules", "__pycache__", "probes", ".git"];

pub fn gather(root: &Path) -> Gathered {
    let base = root.join(DIR);
    if !base.is_dir() {
        return Gathered {
            docs: Vec::new(),
            gaps: vec![Gap {
                source: "scripts",
                count:  0,
                what:   format!("no {DIR} here: scripts not searched"),
            }],
        };
    }
    let mut files = Vec::new();
    walk(&base, &mut files);
    files.sort();
    let mut docs = Vec::new();
    let mut missing = 0;
    for f in files {
        let rel = f.strip_prefix(&base).unwrap_or(&f);
        match describe(root, &f, rel) {
            Some(d) => docs.push(d),
            None => missing += 1,
        }
    }
    let mut gaps = Vec::new();
    if missing > 0 {
        gaps.push(Gap {
            source: "scripts",
            count: missing,
            what: format!(
                "{missing} scripts under {DIR} carry no header comment, so they cannot be found by what they do"
            ),
        });
    }
    Gathered {
        docs,
        gaps,
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !SKIPPED.contains(&name.as_str()) {
                walk(&p, out);
            }
        } else if is_script(&p, &name) {
            out.push(p);
        }
    }
}

/// An executable that is not a test, a fixture or the runner of a suite.
fn is_script(p: &Path, name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    if name.contains("_test") || name.contains("_fixture") || stem == "test" {
        return false;
    }
    executable(p)
}

#[cfg(unix)]
fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(p: &Path) -> bool {
    p.is_file()
}

fn describe(root: &Path, file: &Path, rel: &Path) -> Option<Doc> {
    let text = std::fs::read_to_string(file).ok()?;
    let name = file.file_name()?.to_string_lossy().into_owned();
    let path = file
        .strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned();
    let mut d = parse(&name, &path, &text)?;
    d.place = rel
        .parent()
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    Some(d)
}

/// The comment block a text opens with, one string a line, the comment marks taken off.
pub fn header(text: &str) -> Vec<String> {
    let mut lines = text.lines().peekable();
    while lines
        .peek()
        .is_some_and(|l| l.starts_with("#!") || l.trim().is_empty())
    {
        lines.next();
    }
    let Some(first) = lines.peek().map(|l| l.trim_start().to_string()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(rest) = first
        .strip_prefix("\"\"\"")
        .or_else(|| first.strip_prefix("/**"))
    {
        let close = if first.starts_with("\"\"\"") { "\"\"\"" } else { "*/" };
        lines.next();
        let mut cur = rest.to_string();
        loop {
            if let Some(i) = cur.find(close) {
                cur.truncate(i);
                out.push(cur);
                break;
            }
            out.push(cur);
            match lines.next() {
                Some(l) => cur = l.trim_start().to_string(),
                None => break,
            }
            if !cur.contains(close) {
                cur = cur.trim_start_matches('*').to_string();
            }
        }
    } else if first.starts_with('#') || first.starts_with("//") {
        let mark = if first.starts_with('#') { '#' } else { '/' };
        for l in lines {
            let t = l.trim_start();
            if !t.starts_with(mark) {
                break;
            }
            out.push(t.trim_start_matches(mark).to_string());
        }
    }
    out.into_iter()
        .filter(|l| !l.trim_start().starts_with("shellcheck"))
        .collect()
}

fn is_rule(l: &str) -> bool {
    let t = l.trim();
    t.is_empty()
        || t.chars()
            .all(|c| matches!(c, '=' | '-' | '#' | '*' | '/' | '_'))
}

/// The script described by its text, or nothing where the block has no prose.
pub fn parse(name: &str, path: &str, text: &str) -> Option<Doc> {
    let lines = header(text);
    let stem = name.split('.').next().unwrap_or(name);
    let mut summary: Option<String> = None;
    let mut body: Vec<String> = Vec::new();
    let mut tags = Vec::new();
    let mut verbs: Vec<Verb> = Vec::new();
    for l in &lines {
        let t = l.trim();
        if is_rule(t) {
            continue;
        }
        if let Some(rest) = t.strip_prefix("tags:") {
            tags.extend(
                rest.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
            continue;
        }
        if let Some(v) = usage(name, stem, path, t) {
            if !verbs.iter().any(|x| x.name == v.name) {
                verbs.push(v);
            }
            continue;
        }
        if summary.is_none() {
            summary = Some(strip_name(t, name, stem));
        } else {
            body.push(t.to_string());
        }
    }
    let summary = summary.filter(|s| !s.is_empty())?;
    let mut d = Doc::new(Kind::Script, stem, path, &summary);
    d.tags = tags;
    d.verbs = verbs;
    d.body = body.join(" ");
    Some(d)
}

fn strip_name(t: &str, name: &str, stem: &str) -> String {
    for n in [name, stem] {
        for sep in [" - ", ": ", " \u{2014} ", ": "] {
            if let Some(r) = t.strip_prefix(n).and_then(|r| r.strip_prefix(sep)) {
                return r.trim().to_string();
            }
        }
    }
    t.to_string()
}

/// A usage line: it begins with the script's own name (or its path), and its second word is a verb.
fn usage(name: &str, stem: &str, path: &str, t: &str) -> Option<Verb> {
    let mut words = t.split_whitespace();
    let first = words.next()?;
    let base = first.rsplit('/').next().unwrap_or(first);
    if base != name && base != stem {
        return None;
    }
    let verb = words.next()?;
    if !verb.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        || !verb
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return None;
    }
    // The usage, then the columns after it: arguments set apart by their own gap are not the summary.
    let summary = t
        .split("  ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .skip(1)
        .find(|s| !s.starts_with(['<', '[', '-']))
        .unwrap_or("")
        .to_string();
    Some(Verb {
        name: verb.to_string(),
        summary,
        command: format!("{path} {verb}"),
    })
}
