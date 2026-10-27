//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The milestone reports over repositories built here, never the live workspace.
//! The range reports: partitions, units and stand-ins.

use super::super::MilestoneError;
use super::super::range::{
    PartitionKind,
    ROOT_UNIT,
    is_read,
    kind_of,
    marker_of,
    partition,
    stand_ins,
    unit_of,
};
use super::Fx;

#[test]
fn units_and_kinds_are_read_off_the_path() {
    assert_eq!(unit_of("mock/crates/homma-core/src/x.rs"), "homma-core");
    assert_eq!(unit_of("crates/a/README.md.tmpl"), "a");
    assert_eq!(unit_of("crates/loose.rs"), ROOT_UNIT);
    assert_eq!(unit_of("scripts/run"), ROOT_UNIT);
    assert_eq!(kind_of("a/README.md"), PartitionKind::Document);
    assert_eq!(kind_of("mock/DESIGN.md.tmpl"), PartitionKind::Document);
    assert_eq!(kind_of("docs/guide.txt"), PartitionKind::Document);
    assert_eq!(kind_of("src/docs/x.rs"), PartitionKind::Source);
    assert_eq!(kind_of("notes.md.bak"), PartitionKind::Source);
}

#[test]
fn a_range_splits_by_crate_and_kind_the_most_changed_first() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.commit("crates/a/src/lib.rs", "1\n2\n3\n");
    fx.commit("crates/a/README.md.tmpl", "doc\n");
    fx.commit("crates/b/src/lib.rs", "1\n2\n3\n4\n5\n");
    fx.commit("docs/guide.md", "g\n");
    fx.commit("script.sh", "s\ns\n");
    std::fs::write(fx.root().join("blob.bin"), [0u8, 159, 146, 150, 0]).unwrap();
    fx.git(&["add", "blob.bin"]);
    fx.git(&["commit", "-qm", "blob"]);
    let parts = partition(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = parts
        .iter()
        .map(|p| (p.unit.as_str(), p.kind, p.files, p.changed()))
        .collect();
    assert_eq!(rows, [
        ("b", PartitionKind::Source, 1, 5),
        ("a", PartitionKind::Source, 1, 3),
        (ROOT_UNIT, PartitionKind::Source, 2, 2),
        (ROOT_UNIT, PartitionKind::Document, 1, 1),
        ("a", PartitionKind::Document, 1, 1),
    ]);
}

#[test]
fn a_range_whose_base_the_head_does_not_contain_is_refused() {
    let fx = Fx::new();
    fx.commit("a.txt", "one\n");
    fx.git(&["switch", "-qc", "side"]);
    let off = fx.commit("b.txt", "side\n");
    fx.git(&["switch", "-q", "dev"]);
    fx.commit("a.txt", "two\n");
    assert!(matches!(
        partition(fx.root(), &off, "dev"),
        Err(MilestoneError::NotAncestor { .. })
    ));
    assert!(matches!(
        stand_ins(fx.root(), &off, "dev"),
        Err(MilestoneError::NotAncestor { .. })
    ));
    assert!(matches!(
        partition(fx.root(), "nowhere", "dev"),
        Err(MilestoneError::NotACommit(_))
    ));
}

#[test]
fn a_marker_is_found_by_the_first_in_the_order_and_case_folds_only_where_declared() {
    assert_eq!(marker_of("// FIXME: x, and relaxed"), Some("FIXME"));
    assert_eq!(marker_of("assert!(a); // Relaxed for now"), Some("relaxed"));
    assert_eq!(marker_of("// A Stand-In for the store"), Some("stand-in"));
    assert_eq!(marker_of("A Stand-In for the store"), None);
    assert_eq!(marker_of("x.load(Ordering::Relaxed)"), None);
    assert_eq!(
        marker_of("x.load(Ordering::Relaxed); // relaxed on purpose"),
        Some("relaxed")
    );
    assert_eq!(marker_of(" * a relaxed assertion"), Some("relaxed"));
    // a continuation is `* ` and text; a dereference at the start of a line is code
    assert_eq!(marker_of("*self.relaxed = true;"), None);
    assert_eq!(marker_of("    *stand_in = 1;"), None);
    assert_eq!(marker_of(" *"), None);
    assert_eq!(marker_of("let s = \"relaxed\";"), None);
    assert_eq!(marker_of("// fixme: lowercase is not the marker"), None);
    assert_eq!(marker_of("let todos = 1;"), None);
    assert_eq!(marker_of("#[ignore = \"slow\"]"), Some("#[ignore"));
    assert_eq!(marker_of("nothing to see"), None);
}

#[test]
fn stand_ins_are_the_added_lines_only_at_their_line_in_the_head() {
    let fx = Fx::new();
    // a marker already there, and one the range removes, are not the range's
    fx.commit(
        "old.rs",
        "// FIXME: before the range\nfn keep() {}\n// TODO: removed later\n",
    );
    let start = fx.git(&["rev-parse", "HEAD"]);
    fx.commit("old.rs", "// FIXME: before the range\nfn keep() {}\n");
    fx.commit(
        "new.rs",
        "fn a() {}\n// FIXME: added\nfn b() { todo!(\"later\") }\n",
    );
    fx.commit(
        "tests/t.rs",
        "#[ignore]\nfn t() {}\n// relaxed from equality\n",
    );
    fx.commit("clean.rs", "fn fine() {}\n");
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.marker.as_str()))
        .collect();
    assert_eq!(rows, [
        ("new.rs", 2, "FIXME"),
        ("new.rs", 3, "todo!("),
        ("tests/t.rs", 1, "#[ignore"),
        ("tests/t.rs", 3, "relaxed"),
    ]);
    assert_eq!(found[0].text, "// FIXME: added");
}

#[test]
fn a_binary_file_is_not_read_for_stand_ins() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    std::fs::write(fx.root().join("b.bin"), b"\0FIXME\0TODO\0").unwrap();
    fx.git(&["add", "b.bin"]);
    fx.git(&["commit", "-qm", "bin"]);
    assert!(stand_ins(fx.root(), &start, "dev").unwrap().is_empty());
}

#[test]
fn stand_ins_are_read_in_source_only_and_a_memory_ordering_is_not_one() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.commit(
        "mock/research/notes.md",
        "FIXME in a note\nTODO in a note\n",
    );
    fx.commit("docs/guide.md", "// relaxed in a page\n");
    fx.commit(
        "src/lib.rs",
        "use std::sync::atomic::Ordering;\nfn n() { c.load(Ordering::Relaxed); }\n// FIXME: real\n",
    );
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.marker.as_str()))
        .collect();
    assert_eq!(rows, [("src/lib.rs", 3, "FIXME")]);
}

#[test]
fn a_placeholder_in_data_and_config_is_read_and_one_in_notes_patches_and_histories_is_not() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    let line = "knob = 1 # FIXME(stand-in): tune later\n";
    fx.commit("content/x.toml", line);
    fx.commit("mock/research/n/x.toml", line);
    fx.commit("mock/design_rounds/202610270000/y.toml", line);
    fx.commit("crates/c/mock/research/z.toml", line);
    fx.commit("mock/research/n/fix.patch", "+// FIXME: quoted\n");
    fx.commit("mock/design_rounds/r/.history", "FIXME in a history\n");
    fx.commit("tools/app.js", "// TODO: code is source\n");
    // a data line with no comment opener is no soft marker, whatever it says
    fx.commit(
        "data/rows.json",
        "{\"a\": \"stand-in\", \"b\": \"relaxed\"}\n",
    );
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.marker.as_str()))
        .collect();
    assert_eq!(rows, [
        ("content/x.toml", "FIXME"),
        ("tools/app.js", "TODO")
    ]);
}

#[test]
fn the_places_stand_ins_are_not_read_in_are_named_by_path_alone() {
    for yes in [
        "src/lib.rs",
        "content/x.toml",
        "a.json",
        "x.tsv",
        "t.js",
        "mock/crates/c/src/a.rs",
    ] {
        assert!(is_read(yes), "{yes}");
    }
    for no in [
        "mock/research/a.toml",
        "mock/design_rounds/a/b.rs",
        "k/mock/research/a.toml",
        "a.patch",
        "x/.history",
        "docs/a.toml",
        "README.md",
        "a.md.tmpl",
    ] {
        assert!(!is_read(no), "{no}");
    }
    // a directory that merely ends in the same words is not the place
    assert!(is_read("notmock/research/a.toml"));
}

#[test]
fn a_users_diff_configuration_changes_no_stand_in() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    fx.git(&["config", "diff.noprefix", "true"]);
    fx.git(&["config", "diff.mnemonicPrefix", "true"]);
    // an external driver that cannot run: without `--no-ext-diff` the diff fails
    fx.git(&["config", "diff.external", "no-such-differ-anywhere"]);
    fx.commit("src/lib.rs", "// FIXME: still found\n");
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, "src/lib.rs");
    assert_eq!(found[0].line, 1);
}

#[test]
fn a_comment_naming_the_memory_ordering_is_no_stand_in_and_unimplemented_is_one() {
    let fx = Fx::new();
    let start = fx.commit("seed.txt", "x\n");
    // the ordering is removed before the word is looked for, in a comment too
    fx.commit("a.rs", "// uses Ordering::Relaxed here\n");
    fx.commit("b.rs", "fn f() { unimplemented!(\"later\") }\n");
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.marker.as_str()))
        .collect();
    assert_eq!(rows, [("b.rs", "unimplemented!(")]);
    assert_eq!(marker_of("// uses Ordering::Relaxed here"), None);
    assert_eq!(
        marker_of("// uses Ordering::Relaxed, relaxed"),
        Some("relaxed")
    );
}

#[test]
fn a_renamed_file_is_a_deletion_and_an_addition_in_both_reports() {
    let fx = Fx::new();
    fx.commit("old.rs", "// TODO: carried along\nfn x() {}\n");
    let start = fx.git(&["rev-parse", "HEAD"]);
    fx.git(&["mv", "old.rs", "new.rs"]);
    fx.git(&["commit", "-qm", "rename"]);
    // renames followed would show neither: the file is identical
    let found = stand_ins(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = found
        .iter()
        .map(|s| (s.file.as_str(), s.line, s.marker.as_str()))
        .collect();
    assert_eq!(rows, [("new.rs", 1, "TODO")]);
    let parts = partition(fx.root(), &start, "dev").unwrap();
    let rows: Vec<_> = parts
        .iter()
        .map(|p| (p.unit.as_str(), p.files, p.added, p.deleted))
        .collect();
    assert_eq!(rows, [(ROOT_UNIT, 2, 2, 2)]);
}
