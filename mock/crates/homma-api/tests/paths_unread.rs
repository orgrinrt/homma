//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The keys under `[paths]` that `Paths` does not read, kept by name and refused by nothing yet.

use homma_api::{Paths, Workspace};

#[test]
fn a_key_under_paths_that_no_field_reads_is_kept_by_name() {
    let w = Workspace::parse(
        "[paths]\ncaptures = \"c\"\nzeta_list = \"z\"\nbackups = \"b\"\nhistory_scan = \"s\"\n",
    )
    .expect("an unread key is refused by nothing yet");
    assert_eq!(w.paths_unread, ["backups", "zeta_list"]);
    assert_eq!(w.paths.captures.as_path(), std::path::Path::new("c"));
}

#[test]
fn a_paths_table_of_keys_it_reads_keeps_none() {
    // Failing control: every key `Paths` reads is one it lists, so none of them is reported.
    let every: Vec<String> = homma_api::keys_of::<Paths>()
        .unwrap()
        .read()
        .iter()
        .cloned()
        .collect();
    assert!(every.iter().any(|k| k == "captures"), "{every:?}");
    let text = every
        .iter()
        .map(|k| format!("{k} = \"x\"\n"))
        .collect::<String>();
    let w = Workspace::parse(&format!("[paths]\n{text}")).expect("every key it reads parses");
    assert!(w.paths_unread.is_empty(), "{:?}", w.paths_unread);
    assert!(Workspace::parse("").unwrap().paths_unread.is_empty());
}

#[test]
fn the_unread_keys_of_a_raw_table_are_sorted_and_a_value_that_is_no_table_has_none() {
    let raw: toml::Value = toml::from_str("b = 1\na = 2\ncaptures = \"c\"").unwrap();
    assert_eq!(Paths::unread(&raw).unwrap(), ["a", "b"]);
    assert!(Paths::unread(&toml::Value::Integer(1)).unwrap().is_empty());
}
