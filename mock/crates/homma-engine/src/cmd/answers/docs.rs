//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The documents a page or a channel holds, read and checked.
//!
//! Whoever can open the page wrote them, so every field that reaches a path, a name or a line of
//! a capture's front matter is checked for its shape before anything is derived from it. A
//! document that fails is named and skipped; the ones beside it are taken.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Which kind of document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    Answers,
    Notes,
}

impl Collection {
    pub const fn word(self) -> &'static str {
        match self {
            Collection::Answers => "answers",
            Collection::Notes => "notes",
        }
    }
}

/// A document that passed its checks, with the key a capture lists it under.
#[derive(Debug, Clone)]
pub struct Document {
    pub collection: Collection,
    pub body:       Value,
    /// `answers/<id>@<at>` or `notes/<digits of at>@<at>`.
    pub key:        String,
    pub at:         String,
}

impl Document {
    pub fn text(&self, field: &str) -> &str {
        self.body[field].as_str().unwrap_or_default()
    }

    /// What the document is called where one name is wanted: the row it answers as
    /// `<namespace>::<id>`, or the plain key a registry question carries.
    pub fn label(&self) -> String {
        match &self.body["key"] {
            Value::Object(m) if m.len() == 1 => {
                m.iter()
                    .next()
                    .map(|(ns, id)| format!("{ns}::{}", id.as_str().unwrap_or_default()))
                    .unwrap_or_default()
            },
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => self.text("id").to_string(),
        }
    }
}

fn ident(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 190
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "_.~:@+-".contains(c))
}

fn digits(s: &str, from: usize, count: usize) -> bool {
    s.get(from .. from + count)
        .is_some_and(|d| d.bytes().all(|b| b.is_ascii_digit()))
}

/// `YYYY-MM-DDTHH:MM`, then optionally `:SS` and `.fraction`, then optionally `Z` or `±HH:MM`.
pub fn is_stamp(s: &str) -> bool {
    let b = s.as_bytes();
    if !(digits(s, 0, 4)
        && b.get(4) == Some(&b'-')
        && digits(s, 5, 2)
        && b.get(7) == Some(&b'-')
        && digits(s, 8, 2)
        && b.get(10) == Some(&b'T')
        && digits(s, 11, 2)
        && b.get(13) == Some(&b':')
        && digits(s, 14, 2))
    {
        return false;
    }
    let mut at = 16;
    if b.get(at) == Some(&b':') {
        if !digits(s, at + 1, 2) {
            return false;
        }
        at += 3;
        if b.get(at) == Some(&b'.') {
            let n = b[at + 1 ..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if n == 0 {
                return false;
            }
            at += 1 + n;
        }
    }
    match &s[at ..] {
        "" | "Z" => true,
        z => {
            let zb = z.as_bytes();
            z.len() == 6
                && (zb[0] == b'+' || zb[0] == b'-')
                && digits(z, 1, 2)
                && zb[3] == b':'
                && digits(z, 4, 2)
        },
    }
}

fn control(c: char) -> bool {
    c.is_control()
}

fn textual(v: &Value) -> bool {
    v.is_null() || v.is_string()
}

/// Why a document may not be used, or nothing when it may.
pub fn fault(collection: Collection, v: &Value) -> Option<String> {
    let Some(map) = v.as_object() else {
        return Some("is not a document".into());
    };
    match map.get("at").and_then(Value::as_str) {
        Some(at) if is_stamp(at) => {},
        _ => return Some("has an `at` that is not a timestamp".into()),
    }
    if !textual(&v["text"]) {
        return Some("has a `text` that is not text".into());
    }
    if collection == Collection::Notes {
        return None;
    }
    if !v["id"].as_str().is_some_and(ident) {
        return Some("has an `id` that is not a plain identifier".into());
    }
    let key_ok = match &v["key"] {
        Value::String(s) => ident(s),
        Value::Number(n) => ident(&n.to_string()),
        Value::Object(m) => {
            m.len() == 1
                && m.iter()
                    .all(|(k, id)| ident(k) && id.as_str().is_some_and(ident))
        },
        _ => false,
    };
    if !key_ok {
        return Some(
            "has a `key` that is neither a plain identifier nor one `{namespace: id}`".into(),
        );
    }
    if !v["kind"].as_str().is_some_and(ident) {
        return Some("has a `kind` that is not a plain name".into());
    }
    if !v["where"].as_str().is_some_and(ident) {
        return Some("has a `where` that is not a plain name".into());
    }
    if !textual(&v["asks"]) || !textual(&v["choice"]) && !v["choice"].is_array() {
        return Some("has an `asks` or `choice` that is not text".into());
    }
    if v["asks"].as_str().is_some_and(|a| {
        a.chars()
            .any(|c| control(c) && !matches!(c, '\t' | '\n' | '\r'))
    }) {
        return Some("has an `asks` with a control character other than a line break".into());
    }
    let choices: Vec<&Value> = match &v["choice"] {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        one => vec![one],
    };
    if choices.iter().any(|c| !c.is_string()) {
        return Some("has a `choice` that is not text".into());
    }
    if choices
        .iter()
        .any(|c| c.as_str().is_some_and(|s| s.chars().any(control)))
    {
        return Some("has a `choice` with a control character".into());
    }
    match &v["options"] {
        Value::Null => {},
        Value::Array(a) => {
            if a.iter().any(|o| !o.is_string()) {
                return Some("has `options` that are not a list of text".into());
            }
            if a.iter()
                .any(|o| o.as_str().is_some_and(|s| s.chars().any(control)))
            {
                return Some("has an entry of `options` with a control character".into());
            }
        },
        _ => return Some("has `options` that are not a list of text".into()),
    }
    None
}

/// The store key of a checked document.
fn key_of(collection: Collection, v: &Value, at: &str) -> String {
    match collection {
        Collection::Answers => format!("answers/{}@{at}", v["id"].as_str().unwrap_or_default()),
        Collection::Notes => {
            let digits: String = at.chars().filter(char::is_ascii_digit).take(17).collect();
            format!("notes/{digits}@{at}")
        },
    }
}

/// One document, or why it was skipped.
pub fn parse(collection: Collection, text: &str) -> Result<Document, String> {
    let body: Value =
        serde_json::from_str(text).map_err(|e| format!("is not readable as JSON: {e}"))?;
    if let Some(why) = fault(collection, &body) {
        return Err(why);
    }
    let at = body["at"].as_str().unwrap_or_default().to_string();
    Ok(Document {
        collection,
        key: key_of(collection, &body, &at),
        body,
        at,
    })
}

/// What a read found: the documents that passed, and a line for each that did not.
#[derive(Debug, Default)]
pub struct Read {
    pub taken:   Vec<Document>,
    pub skipped: Vec<String>,
}

/// The documents under `dir/answers/` and `dir/notes/`, one JSON file each.
pub fn read_dir(dir: &Path) -> Result<Read> {
    let mut out = Read::default();
    for collection in [Collection::Answers, Collection::Notes] {
        let sub = dir.join(collection.word());
        if !sub.is_dir() {
            continue;
        }
        let mut files: Vec<_> = std::fs::read_dir(&sub)
            .with_context(|| format!("reading {}", sub.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json") && p.is_file())
            .collect();
        files.sort();
        for f in files {
            let text =
                std::fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?;
            match parse(collection, &text) {
                Ok(d) => out.taken.push(d),
                Err(why) => out.skipped.push(format!("{} {why}", f.display())),
            }
        }
    }
    Ok(out)
}

/// The tool the channel is read with: the one `HOMMA_MSG` names, else the workspace's own `msg`,
/// else the one on the path. Read once, by the command, and handed down.
pub fn msg_program(root: &Path) -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("HOMMA_MSG").filter(|p| !p.is_empty()) {
        return p.into();
    }
    let own = root.join("tools/msg/msg");
    if own.is_file() { own } else { "msg".into() }
}

/// The `answer` and `note` messages `from` sent on `channel`, each body read as a document.
pub fn read_channel(root: &Path, msg: &Path, channel: &str, from: &str) -> Result<Read> {
    let out = Command::new(msg)
        .current_dir(root)
        .args([channel, "show", "--from", from, "-n", "1000", "--json"])
        .output()
        .with_context(|| format!("running msg for the channel {channel}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        if stdout.contains(&format!("nothing from {from}"))
            || stderr.contains(&format!("nothing from {from}"))
        {
            return Ok(Read::default());
        }
        bail!("the channel {channel} could not be read: {}", stderr.trim());
    }
    let listing: Value = serde_json::from_str(&stdout).context("msg did not print JSON")?;
    let mut read = Read::default();
    for m in listing["messages"].as_array().into_iter().flatten() {
        let collection = match m["kind"].as_str() {
            Some("answer") => Collection::Answers,
            Some("note") => Collection::Notes,
            _ => continue,
        };
        let id = m["id"].as_str().unwrap_or("a message");
        match parse(collection, m["body"].as_str().unwrap_or_default()) {
            Ok(d) => read.taken.push(d),
            Err(why) => {
                read.skipped
                    .push(format!("the {} message {id} {why}", collection.word()))
            },
        }
    }
    Ok(read)
}
