//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The gate against a runner that answers from a table, so every step can be
//! exercised without cargo or deno installed.

//!
//! Three modules, one per thing being asserted: the feature-set matrix, the
//! steps one at a time, and the whole gate.

mod features;
mod steps;
mod whole;

use std::cell::RefCell;

use homma_api::Verdict;

use super::*;

/// Answers each command line from a table and records what was asked.
struct Fake {
    replies: Vec<(&'static str, i32, &'static str)>,
    seen:    RefCell<Vec<String>>,
}

impl Fake {
    fn new(replies: Vec<(&'static str, i32, &'static str)>) -> Self {
        Self {
            replies,
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for Fake {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[&str],
        _env: &[(&str, &str)],
    ) -> Result<sh::Output, sh::Spawn> {
        let line = format!("{program} {}", args.join(" "));
        self.seen.borrow_mut().push(line.clone());
        let (status, stdout) = self
            .replies
            .iter()
            .find(|(prefix, ..)| line.starts_with(prefix))
            .map(|(_, s, o)| (*s, *o))
            .unwrap_or((0, ""));
        Ok(sh::Output {
            program: program.into(),
            args:    args.iter().map(|a| a.to_string()).collect(),
            status:  Some(status),
            stdout:  stdout.into(),
            stderr:  String::new(),
        })
    }
}

fn crate_root(manifest: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("Cargo.toml"), manifest).unwrap();
    d
}

fn git_repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for (name, body) in files {
        std::fs::write(d.path().join(name), body).unwrap();
    }
    let run = |args: &[&str]| {
        let out = sh::run(d.path(), "git", args).unwrap();
        assert!(out.ok(), "{}", out.log());
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
    run(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"]);
    d
}
