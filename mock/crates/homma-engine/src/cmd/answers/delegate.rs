//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What mockspace's `registry-append` hook runs. It takes the step whole for one thing, words
//! appended to a row's `answers`, and leaves every other write to mockspace by declining.

use std::path::Path;

use anyhow::Result;
use serde_json::json;

use super::docs::{self, Collection};
use super::{Run, Source, take_documents};

/// What the delegate did.
#[derive(Debug, PartialEq, Eq)]
pub enum Delegated {
    /// The words are a capture and the row holds a pointer to it.
    Whole,
    /// Not a step for this command; mockspace writes the row.
    Declined,
}

/// The value of an `answers` entry made of words: `<at>: <words>`.
fn words(value: &str) -> Option<(&str, &str)> {
    let (at, said) = value.split_once(": ")?;
    (docs::is_stamp(at) && !said.trim().is_empty()).then_some((at, said))
}

/// The step `registry-append <namespace>::<id> <field> <value>` asked for.
pub fn run(run: &Run, repo: &Path, target: &str, field: &str, value: &str) -> Result<Delegated> {
    let Some((namespace, id)) = target.split_once("::") else {
        return Ok(Delegated::Declined);
    };
    let Some((at, said)) = (field == "answers").then(|| words(value)).flatten() else {
        return Ok(Delegated::Declined);
    };
    let place = repo
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let document = json!({
        "id": format!("{namespace}.{id}"),
        "key": { namespace: id },
        "kind": "registry",
        "where": place,
        "asks": format!("Words attached to {target} with registry-append"),
        "options": [],
        "choice": null,
        "text": said,
        "at": at,
    });
    let Ok(doc) = docs::parse(Collection::Answers, &document.to_string()) else {
        return Ok(Delegated::Declined);
    };
    let read = docs::Read {
        taken:   vec![doc],
        skipped: Vec::new(),
    };
    let source = Source::Delegated(target);
    take_documents(run, read, &source, Some(repo))?;
    Ok(Delegated::Whole)
}
