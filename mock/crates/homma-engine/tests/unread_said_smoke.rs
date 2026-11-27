//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! An unread manifest entry is said once on stderr by every verb that loads the manifest,
//! besides `homma status`, which says it in its report.

mod support;
use support::bin;

const MANIFEST: &str = "homma_branch = \"dev\"\n[workspace]\nname = \"w\"\npath = \".\"\n";

fn stderr_of(root: &std::path::Path, args: &[&str]) -> String {
    let out = bin().arg("--dir").arg(root).args(args).output().unwrap();
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_verb_that_does_not_read_the_entry_says_it_once_on_stderr() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("homma.toml"),
        format!("{MANIFEST}[later]\nx = 1\n"),
    )
    .unwrap();
    let said = stderr_of(d.path(), &["hook", "doctor"]);
    assert_eq!(
        said.matches("which this engine does not read").count(),
        1,
        "{said}"
    );
    assert!(said.contains("`later`"), "{said}");
    // the status says it in its report and not again on stderr
    assert!(!stderr_of(d.path(), &["status"]).contains("which this engine does not read"));
    // Failing control: a manifest with nothing unread says nothing of it.
    std::fs::write(d.path().join("homma.toml"), MANIFEST).unwrap();
    assert!(!stderr_of(d.path(), &["hook", "doctor"]).contains("does not read"));
}
