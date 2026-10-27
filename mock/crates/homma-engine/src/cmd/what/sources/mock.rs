//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The commands `cargo mock tools` lists, in each mockspace repository of the workspace.
//!
//! The one source that starts a process. It is run for the repositories the instance's scope names
//! where it names any, and a repository where it fails is said in the footer. The builtin subcommands
//! are the same in every repository and are taken once.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::super::doc::{Doc, Gap, Gathered, Kind};

/// How long one repository's listing may take.
const LIMIT: Duration = Duration::from_secs(60);

/// The repositories to ask: those directly under the root with a `mockspace.toml`, within `scope`
/// where it is not empty.
pub fn repos(root: &Path, scope: &[String]) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = read
        .flatten()
        .filter(|e| e.path().join("mockspace.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| scope.is_empty() || scope.contains(n))
        .collect();
    out.sort();
    out
}

pub fn gather(root: &Path, scope: &[String]) -> Gathered {
    let mut out = Gathered::default();
    let mut builtins_done = false;
    for repo in repos(root, scope) {
        match list(&root.join(&repo)) {
            Ok(text) => {
                for mut d in parse(&text, &repo) {
                    if d.tags.iter().any(|t| t == "builtin") {
                        if builtins_done {
                            continue;
                        }
                        d.place.clear();
                    }
                    out.docs.push(d);
                }
                builtins_done = true;
            },
            Err(why) => {
                out.gaps.push(Gap {
                    source: "mock",
                    count:  1,
                    what:   format!("mockspace commands not listed in {repo}: {why}"),
                });
            },
        }
    }
    out
}

/// What `cargo mock tools` prints in `dir`.
fn list(dir: &Path) -> Result<String, String> {
    let mut child = Command::new("cargo")
        .args(["mock", "tools"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cargo mock could not start: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("cargo mock tools took longer than a minute".to_string());
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it exited non-zero");
        return Err(format!("cargo mock tools failed: {}", line.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The commands in a listing: `    mock <usage>  [<kind>  ]<summary>` under a heading in capitals.
pub fn parse(text: &str, repo: &str) -> Vec<Doc> {
    let mut docs = Vec::new();
    let mut project = false;
    for l in text.lines() {
        if !l.starts_with(' ') && !l.trim().is_empty() {
            project = l.trim().starts_with("PROJECT");
            continue;
        }
        let Some(rest) = l.trim_start().strip_prefix("mock ") else {
            continue;
        };
        let parts: Vec<&str> = rest
            .split("  ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let Some(usage) = parts.first() else {
            continue;
        };
        let name = usage.split_whitespace().next().unwrap_or(usage);
        let (kind, summary) = match (project, parts.len()) {
            (true, n) if n >= 3 => (parts[1], parts[2 ..].join(" ")),
            (_, n) if n >= 2 => ("", parts[1 ..].join(" ")),
            _ => ("", String::new()),
        };
        let mut d = Doc::new(Kind::Mock, name, &format!("cargo mock {usage}"), &summary);
        if project {
            d.place = vec![repo.to_string()];
            if !kind.is_empty() {
                d.tags.push(kind.to_string());
            }
        } else {
            d.tags.push("builtin".to_string());
        }
        docs.push(d);
    }
    docs
}
