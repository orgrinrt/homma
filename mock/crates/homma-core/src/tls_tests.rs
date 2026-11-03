//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Every HTTP agent in the crate starts from `tls::agent_builder`, which is where the roots are set.
//!
//! The check reads the crate's source and works from a list of what code outside `src/tls.rs` may name
//! from `ureq`: types and errors, none of which makes an agent. Anything else it names is reported, a
//! constructor or a free request function or a glob or an alias of the crate, so a spelling nobody
//! thought of is caught too. It does not see an agent made through another crate that wraps `ureq`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::no_roots;

/// The heading of the text for a store with no root, which the request test looks for on standard error.
const HEADING: &str =
    "homma: no trusted root certificate could be read, so every HTTPS request will be refused";

/// What outside code may take from `ureq`.
const ALLOWED: &[&str] = &[
    "Agent",
    "Request",
    "Response",
    "Error",
    "ErrorKind",
    "Transport",
    "OrAnyStatus",
    "RedirectAuthHeaders",
    "json",
];

/// Every `.rs` file under `dir` that is not a test file.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("the source directory is readable") {
        let path = entry.expect("a directory entry reads").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs")
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("_tests.rs"))
        {
            out.push(path);
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The identifier a text starts with, or nothing.
fn head_ident(text: &str) -> &str {
    let end = text.find(|c: char| !is_ident(c)).unwrap_or(text.len());
    &text[.. end]
}

/// What `code` does with `ureq` that is not on the list, one reason each. Comment lines are skipped.
fn off_the_list(code: &str) -> Vec<String> {
    let code: String = code
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut reasons = Vec::new();
    let mut from = 0;
    while let Some(found) = code[from ..].find("ureq") {
        let at = from + found;
        from = at + 4;
        let before = code[.. at].chars().next_back();
        let after = &code[at + 4 ..];
        // a word of its own, and not a part of `map_ureq_error`
        if before.is_some_and(is_ident) || after.chars().next().is_some_and(is_ident) {
            continue;
        }
        let Some(path) = after.strip_prefix("::") else {
            reasons.push("names the crate without a path, which is an alias".to_owned());
            continue;
        };
        if let Some(list) = path.strip_prefix('{') {
            let list = &list[.. list.find('}').unwrap_or(list.len())];
            for item in list.split(',').map(str::trim).filter(|i| !i.is_empty()) {
                if !ALLOWED.contains(&head_ident(item)) {
                    reasons.push(format!("takes `{item}`"));
                }
            }
        } else if !ALLOWED.contains(&head_ident(path)) {
            let named = head_ident(path);
            reasons.push(format!(
                "takes `{}`",
                if named.is_empty() { "*" } else { named }
            ));
        }
    }
    // `Agent` is allowed as a type, and `Agent::new` makes one from the bundled roots
    if code.contains("Agent::") {
        reasons.push("makes an `Agent` by a path".to_owned());
    }
    reasons
}

/// The files under `src` that make an agent a way the list does not allow, each with its reasons. Only
/// `tls.rs` directly under `src` is exempt, by its path and not by its name.
fn outside_the_list(src: &Path) -> Vec<(String, Vec<String>)> {
    let mut files = Vec::new();
    sources(src, &mut files);
    files.sort();
    files
        .into_iter()
        .filter(|f| f != &src.join("tls.rs"))
        .filter_map(|f| {
            let reasons = off_the_list(&fs::read_to_string(&f).expect("a source file reads"));
            let name = f
                .strip_prefix(src)
                .expect("under the source directory")
                .display()
                .to_string();
            (!reasons.is_empty()).then_some((name, reasons))
        })
        .collect()
}

/// What the text for an empty store carries: the heading alone, then a line for each variable that is set
/// and for each thing the loader said, and a variable that is not set gets no line. The request test sees
/// the path through the loader's own error as much as through the variable, so it cannot tell a text that
/// leaves a variable out from one that keeps it; this one can.
#[test]
fn the_text_for_a_store_with_no_root_names_each_variable_set_and_what_the_loader_said() {
    assert_eq!(no_roots(&[], None, None), HEADING);

    let file_only = no_roots(&[], Some(OsStr::new("/a/b.pem")), None);
    assert_eq!(file_only, format!("{HEADING}\n  SSL_CERT_FILE is /a/b.pem"));

    let dir_only = no_roots(&[], None, Some(OsStr::new("/c")));
    assert_eq!(dir_only, format!("{HEADING}\n  SSL_CERT_DIR is /c"));

    let all = no_roots(
        &["no such file".to_owned(), "not a certificate".to_owned()],
        Some(OsStr::new("/a/b.pem")),
        Some(OsStr::new("/c")),
    );
    assert_eq!(
        all,
        format!(
            "{HEADING}\n  SSL_CERT_FILE is /a/b.pem\n  SSL_CERT_DIR is /c\n  no such file\n  not a certificate"
        )
    );
}

#[test]
fn no_agent_is_made_outside_the_module_that_sets_the_roots() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(outside_the_list(&src), Vec::new());
}

/// The control: one planted file for each way an agent can be made that the check is there to see, and
/// files it must leave alone, so the test above is a result and not a scan that matches nothing.
#[test]
fn the_check_names_each_way_to_make_an_agent_and_leaves_the_rest() {
    let scratch = tempfile::tempdir().expect("a scratch directory is made");
    let dir = scratch.path();
    fs::create_dir_all(dir.join("forge")).expect("a directory is made");
    let planted: &[(&str, &str)] = &[
        ("forge/new.rs", "let a = ureq::AgentBuilder::new().build();"),
        ("forge/builder.rs", "let a = ureq::builder().build();"),
        ("forge/agent.rs", "let a = ureq::agent();"),
        ("forge/get.rs", "let r = ureq::get(\"https://x\").call();"),
        (
            "forge/request.rs",
            "let r = ureq::request(\"GET\", url).call();",
        ),
        (
            "forge/renamed.rs",
            "use ureq::AgentBuilder as Make;\nlet a = Make::new().build();",
        ),
        (
            "forge/braced.rs",
            "use ureq::{\n    Error,\n    AgentBuilder,\n};",
        ),
        ("forge/glob.rs", "use ureq::*;"),
        ("forge/alias.rs", "use ureq as net;"),
        (
            "forge/agent_new.rs",
            "use ureq::Agent;\nlet a = Agent::new();",
        ),
        // exempt by path at the top of the directory only, so a file of that name below it is not
        ("forge/tls.rs", "let a = ureq::builder().build();"),
    ];
    let clean: &[(&str, &str)] = &[
        ("tls.rs", "ureq::AgentBuilder::new()"),
        (
            "forge/clean.rs",
            "use ureq::{Error, Response};\nfn f(e: ureq::Error, r: ureq::Response, a: ureq::Agent) {}\n\
             fn map_ureq_error() {}\n// ureq::builder() is a comment\nlet j = ureq::json!({});",
        ),
        ("forge/bare_tests.rs", "ureq::builder()"),
    ];
    for (path, body) in planted.iter().chain(clean) {
        fs::write(dir.join(path), body).expect("a file writes");
    }
    let named: Vec<String> = outside_the_list(dir)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let mut want: Vec<String> = planted.iter().map(|(p, _)| (*p).to_owned()).collect();
    want.sort();
    assert_eq!(named, want);
}
