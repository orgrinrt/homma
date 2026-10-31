//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use toml::Table;

use super::*;

fn t(s: &str) -> Table {
    toml::from_str(s).expect("test toml")
}

fn run(base: &str, local: &str, unset: &[&str]) -> (Table, Provenance) {
    let unset: Vec<String> = unset.iter().map(|s| (*s).to_owned()).collect();
    merge("p", &t(base), &t(local), &unset).expect("merges")
}

#[test]
fn a_scalar_in_both_takes_the_local_value() {
    let (m, p) = run("[a]\nx = 1\n", "[a]\nx = 2\n", &[]);
    assert_eq!(m, t("[a]\nx = 2\n"));
    assert_eq!(p.overridden, ["a.x"]);
    assert!(p.inherited.is_empty() && p.local.is_empty());
}

#[test]
fn tables_in_both_merge_key_by_key() {
    let (m, p) = run("[a]\nx = 1\ny = 2\n", "[a]\ny = 3\nz = 4\n", &[]);
    assert_eq!(m, t("[a]\nx = 1\ny = 3\nz = 4\n"));
    assert_eq!(p.inherited, ["a.x"]);
    assert_eq!(p.overridden, ["a.y"]);
    assert_eq!(p.local, ["a.z"]);
}

#[test]
fn an_array_in_both_is_replaced_whole_and_never_interleaved() {
    let (m, p) = run("a = [1, 2, 3]\n", "a = [9]\n", &[]);
    assert_eq!(m, t("a = [9]\n"));
    assert_eq!(p.overridden, ["a"]);
}

#[test]
fn an_empty_local_array_overrides_a_base_array() {
    let (m, _) = run("a = [1]\n", "a = []\n", &[]);
    assert_eq!(m, t("a = []\n"));
}

#[test]
fn a_key_in_one_side_only_is_kept() {
    let (m, p) = run("a = 1\n", "b = 2\n", &[]);
    assert_eq!(m, t("a = 1\nb = 2\n"));
    assert_eq!(
        (p.inherited, p.local),
        (vec!["a".to_owned()], vec!["b".to_owned()])
    );
}

#[test]
fn a_scalar_over_a_table_takes_the_local_one_whole() {
    let (m, p) = run("[a]\nx = 1\ny = 2\n", "a = \"gone\"\n", &[]);
    assert_eq!(m, t("a = \"gone\"\n"));
    assert_eq!(p.overridden, ["a"]);
    assert!(p.inherited.is_empty());
}

#[test]
fn a_table_over_a_scalar_takes_the_local_one_whole() {
    let (m, p) = run("a = 1\n", "[a]\nx = 1\n", &[]);
    assert_eq!(m, t("[a]\nx = 1\n"));
    assert_eq!(p.overridden, ["a.x"]);
}

#[test]
fn unset_removes_an_inherited_scalar_array_and_table() {
    let base = "s = 1\narr = [1]\n[tab]\nx = 1\ny = 2\n[keep]\nz = 1\n";
    let (m, p) = run(base, "", &["s", "arr", "tab"]);
    assert_eq!(m, t("[keep]\nz = 1\n"));
    assert_eq!(p.unset, ["arr", "s", "tab"]);
    assert_eq!(p.inherited, ["keep.z"]);
}

#[test]
fn unset_reaches_into_a_table_and_leaves_its_siblings() {
    let (m, _) = run("[a]\nx = 1\ny = 2\n", "", &["a.x"]);
    assert_eq!(m, t("[a]\ny = 2\n"));
}

#[test]
fn unset_of_a_path_the_base_lacks_is_no_error_and_is_not_reported() {
    let (m, p) = run("a = 1\n", "", &["nope", "a.deeper", "x.y.z"]);
    assert_eq!(m, t("a = 1\n"));
    assert!(p.unset.is_empty());
}

#[test]
fn the_local_file_may_set_an_unset_path_again_and_it_counts_as_local() {
    let (m, p) = run("[a]\nx = 1\n", "[a]\nx = 5\n", &["a.x"]);
    assert_eq!(m, t("[a]\nx = 5\n"));
    assert_eq!(p.local, ["a.x"]);
    assert_eq!(p.unset, ["a.x"]);
    assert!(p.overridden.is_empty());
}

#[test]
fn a_path_that_is_not_one_or_is_the_mechanism_is_refused() {
    for bad in ["", "a..b", ".a", "instance", "instance.project", "instance.unset"] {
        let r = merge("p", &t(""), &t(""), &[bad.to_owned()]);
        assert_eq!(r, Err(BadUnset(bad.to_owned())), "{bad:?}");
    }
}

#[test]
fn the_mechanism_keys_are_not_counted_as_values_the_file_sets() {
    let (m, p) = run(
        "[instance]\nwork = \"p\"\n",
        "[instance]\nproject = \"p\"\nunset = [\"x\"]\n",
        &["x"],
    );
    assert_eq!(
        m,
        t("[instance]\nwork = \"p\"\nproject = \"p\"\nunset = [\"x\"]\n")
    );
    assert!(p.local.is_empty() && p.overridden.is_empty());
    assert_eq!(p.inherited, ["instance.work"]);
}

#[test]
fn an_empty_local_file_inherits_everything() {
    let (m, p) = run("a = 1\n[t]\nb = 2\n", "", &[]);
    assert_eq!(m, t("a = 1\n[t]\nb = 2\n"));
    assert_eq!(p.inherited, ["a", "t.b"]);
}
