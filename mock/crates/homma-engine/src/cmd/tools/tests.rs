//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The listing says what is there, the merged and shadowed built-ins are named as such, and `check`
//! fails while a descriptor is broken.

use super::check;
use super::host::{ToolsReport, report};
use crate::cli::OutputFormat;
use crate::cmd::Outcome;
use crate::output::HumanRender;

fn tool_text(name: &str) -> String {
    format!(
        "[tool]\nname=\"{name}\"\nsummary=\"{name} summary\"\nbackend=\"local\"\npromote=true\n[tool.source]\npath={{path=\"tools/{name}\"}}\n[[tool.commands]]\nname=\"{name}\"\nsummary=\"s\"\nrun=\"{name}\"\n"
    )
}

fn workspace(tools: &[(&str, Option<String>)]) -> tempfile::TempDir {
    let ws = tempfile::tempdir().unwrap();
    for (name, text) in tools {
        let d = ws.path().join("tools").join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("tool.toml"),
            text.clone().unwrap_or_else(|| tool_text(name)),
        )
        .unwrap();
    }
    ws
}

fn rendered(r: &ToolsReport) -> String {
    let mut out = Vec::new();
    r.render_human(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn the_listing_carries_the_summary_and_verbs_and_marks_a_tool_that_is_not_promoted() {
    let ws = workspace(&[
        ("msg", None),
        (
            "quiet",
            Some(tool_text("quiet").replace("promote=true", "promote=false")),
        ),
    ]);
    let text = rendered(&report(ws.path()));
    assert!(text.contains("msg    msg summary  [msg]\n"), "{text}");
    assert!(
        text.contains("quiet  quiet summary  [quiet]  (not promoted: `homma tools run quiet ...`)"),
        "{text}"
    );
}

#[test]
fn a_built_in_of_the_same_name_is_merged_for_a_group_in_the_list_and_shadowing_otherwise() {
    let ws = workspace(&[("rules", None), ("status", None), ("msg", None)]);
    let r = report(ws.path());
    let rel = |n: &str| r.tools.iter().find(|t| t.name == n).unwrap().builtin;
    assert_eq!(rel("rules"), Some("merged"));
    assert_eq!(rel("status"), Some("shadowed"));
    assert_eq!(rel("msg"), None);
    let text = rendered(&r);
    assert!(text.contains("merged with the built-in"), "{text}");
    assert!(
        text.contains("a built-in holds the name: `homma tools run status ...`"),
        "{text}"
    );
}

#[test]
fn a_broken_descriptor_is_listed_after_the_tools_and_no_tools_says_so() {
    let ws = workspace(&[("msg", None), ("bad", Some("[[[[".to_string()))]);
    let text = rendered(&report(ws.path()));
    assert!(
        text.lines().count() == 2 && text.lines().nth(1).unwrap().contains("BROKEN"),
        "{text}"
    );
    assert!(
        text.contains("bad/tool.toml") || text.contains("bad"),
        "{text}"
    );
    assert!(rendered(&report(tempfile::tempdir().unwrap().path())).starts_with("no tools"));
}

#[test]
fn check_is_a_failure_while_a_descriptor_is_broken_and_clean_otherwise() {
    let good = workspace(&[("msg", None)]);
    assert!(matches!(
        check(good.path(), OutputFormat::Human).unwrap(),
        Outcome::Ok
    ));
    let bad = workspace(&[("msg", None), ("bad", Some("[[[[".to_string()))]);
    assert!(matches!(
        check(bad.path(), OutputFormat::Human).unwrap(),
        Outcome::ReportedFailure
    ));
}
