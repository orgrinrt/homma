//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! An answer attached to the row it answers, as `<at>: <capture path>` in the row's `answers`.
//!
//! The write is mockspace's `registry-append`, always with `--local`: the row is written by the
//! one place that edits registry text, and the delegate hook that may have called this is not
//! asked again.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::docs::{Collection, Document};

/// The row an answer belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub namespace: String,
    pub id:        String,
}

impl Target {
    pub fn spelled(&self) -> String {
        format!("{}::{}", self.namespace, self.id)
    }
}

/// The row a document answers: `{"ticket": "<id>"}` names `ticket::<id>`, and a bare id on a
/// `registry` question names the `question` row of that id. An agenda question and a note answer
/// no row.
pub fn target(d: &Document) -> Option<Target> {
    if d.collection != Collection::Answers {
        return None;
    }
    match &d.body["key"] {
        Value::Object(m) if m.len() == 1 => {
            let (ns, id) = m.iter().next()?;
            Some(Target {
                namespace: ns.clone(),
                id:        id.as_str()?.to_string(),
            })
        },
        Value::String(id) if d.text("kind") == "registry" => {
            Some(Target {
                namespace: "question".into(),
                id:        id.clone(),
            })
        },
        Value::Number(n) if d.text("kind") == "registry" => {
            Some(Target {
                namespace: "question".into(),
                id:        n.to_string(),
            })
        },
        _ => None,
    }
}

/// The clone the row lives in: the one the document's `where` names when it is a clone in the
/// workspace, else the work's own repository.
pub fn repo_for(root: &Path, place: &str, work: Option<&str>) -> Option<PathBuf> {
    [Some(place), work]
        .into_iter()
        .flatten()
        .map(|name| root.join(name))
        .find(|p| p.join(".git").exists())
}

/// How mockspace is run: a launcher that takes the subcommand directly where one is named,
/// else `cargo mock`.
fn launcher(mock: Option<&Path>) -> Command {
    match mock {
        Some(p) => Command::new(p),
        None => {
            let mut c = Command::new("cargo");
            c.arg("mock");
            c
        },
    }
}

/// Adds `<at>: <capture>` to the `answers` of `target` in `repo`.
pub fn row(
    mock: Option<&Path>,
    repo: &Path,
    target: &Target,
    at: &str,
    capture: &str,
) -> Result<(), String> {
    let out = launcher(mock)
        .current_dir(repo)
        .args([
            "registry-append",
            &target.spelled(),
            "answers",
            &format!("{at}: {capture}"),
            "--local",
        ])
        .output()
        .map_err(|e| format!("could not run mockspace in {}: {e}", repo.display()))?;
    if out.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let first = said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("it gave no reason");
    Err(format!(
        "{} in {}: {}",
        target.spelled(),
        repo.display(),
        first.trim()
    ))
}
