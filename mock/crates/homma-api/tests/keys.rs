//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `keys_of`: every key a type reads, found with no manifest.

use std::collections::BTreeMap;

use homma_api::{Keys, WorkspaceName, keys_of};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    workspace: Named,
    #[serde(default)]
    plain:     Option<String>,
    #[serde(default)]
    nested:    Nested,
    #[serde(default)]
    forges:    BTreeMap<String, Forge>,
    #[serde(default)]
    steps:     Vec<Step>,
    #[serde(default)]
    words:     Vec<String>,
    #[serde(default)]
    raw:       Option<toml::Value>,
    #[serde(default)]
    either:    Option<Either>,
    #[serde(default, rename = "spelt-so")]
    renamed:   bool,
    #[serde(skip_deserializing)]
    hidden:    u32,
}

#[derive(Deserialize)]
struct Named {
    name: WorkspaceName,
}

#[derive(Deserialize, Default)]
struct Nested {
    #[serde(default)]
    depth: u8,
    #[serde(default)]
    inner: Option<Inner>,
}

#[derive(Deserialize, Default)]
struct Inner {
    #[serde(default)]
    leaf: f64,
}

#[derive(Deserialize)]
struct Forge {
    kind: Kind,
    url:  String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    One,
    Two,
}

#[derive(Deserialize)]
struct Step {
    run: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Either {
    Bare(String),
    Table {
        path: String,
    },
}

fn read(keys: &Keys) -> Vec<&str> {
    keys.read().iter().map(String::as_str).collect()
}

#[test]
fn every_key_a_type_reads_is_listed_by_its_dotted_path() {
    let keys = keys_of::<Manifest>().expect("the probe reads the type");
    assert_eq!(read(&keys), [
        "either",
        "forges.*.kind",
        "forges.*.url",
        "nested.depth",
        "nested.inner.leaf",
        "plain",
        "raw",
        "spelt-so",
        "steps.run",
        "words",
        "workspace.name",
    ]);
}

#[test]
fn a_key_read_as_any_value_is_held_apart_as_opaque() {
    let keys = keys_of::<Manifest>().unwrap();
    let opaque: Vec<&str> = keys.opaque().iter().map(String::as_str).collect();
    assert_eq!(opaque, ["either", "raw"]);
}

#[derive(Deserialize)]
struct ReadsRaw {
    #[serde(default)]
    first: Option<String>,
    #[serde(default)]
    raw:   Option<toml::Value>,
}

#[derive(Deserialize)]
struct RawsReader {
    #[serde(default)]
    deep: Option<String>,
}

#[derive(Deserialize)]
struct AtTheRoot {
    #[serde(default)]
    raw:    Option<Inner>,
    #[serde(default)]
    second: Option<String>,
}

#[test]
fn a_graft_under_an_opaque_key_replaces_it_with_the_keys_under_it() {
    let mut keys = keys_of::<ReadsRaw>().unwrap();
    keys.graft("raw", keys_of::<RawsReader>().unwrap());
    assert_eq!(read(&keys), ["first", "raw.deep"]);
    assert!(keys.opaque().is_empty());
}

#[test]
fn a_graft_at_the_root_covers_the_opaque_keys_it_reads_tables_under() {
    let mut keys = keys_of::<ReadsRaw>().unwrap();
    keys.graft("", keys_of::<AtTheRoot>().unwrap());
    assert_eq!(read(&keys), ["first", "raw.leaf", "second"]);
    assert!(keys.opaque().is_empty());
}

#[test]
fn a_graft_of_another_opaque_key_leaves_it_opaque() {
    let mut keys = keys_of::<ReadsRaw>().unwrap();
    keys.graft("", keys_of::<ReadsRaw>().unwrap());
    let opaque: Vec<&str> = keys.opaque().iter().map(String::as_str).collect();
    assert_eq!(opaque, ["raw"]);
}

#[test]
fn a_manifest_setting_every_listed_key_is_read() {
    // The keys listed are the ones the type takes: a manifest setting each of them parses.
    let text = r#"
        either = { path = "p" }
        plain = "p"
        raw = { anything = 1 }
        spelt-so = true
        words = ["w"]
        [workspace]
        name = "w"
        [nested]
        depth = 2
        inner = { leaf = 1.5 }
        [forges.one]
        kind = "two"
        url = "u"
        [[steps]]
        run = ["r"]
    "#;
    let Manifest {
        workspace: Named {
            name,
        },
        plain,
        nested: Nested {
            depth,
            inner,
        },
        forges,
        steps,
        words,
        raw,
        either,
        renamed,
        hidden,
    } = toml::from_str(text).expect("a manifest of the listed keys parses");
    assert_eq!(name, "w");
    assert_eq!(plain.as_deref(), Some("p"));
    assert_eq!((depth, inner.map(|i| i.leaf)), (2, Some(1.5)));
    let forge = &forges["one"];
    assert!(matches!(forge.kind, Kind::Two) && forge.url == "u");
    assert!(!matches!(forge.kind, Kind::One));
    assert_eq!(steps[0].run, ["r"]);
    assert_eq!(words, ["w"]);
    assert!(raw.is_some());
    assert!(matches!(either, Some(Either::Table { ref path }) if path == "p"));
    assert!(!matches!(either, Some(Either::Bare(_))));
    assert!(renamed);
    assert_eq!(hidden, 0);
}

#[test]
fn the_types_grafted_read_what_they_list() {
    let r: RawsReader = toml::from_str("deep = \"d\"").unwrap();
    assert_eq!(r.deep.as_deref(), Some("d"));
    let a: AtTheRoot = toml::from_str("second = \"s\"\nraw = { leaf = 2.0 }").unwrap();
    assert_eq!(
        (a.second.as_deref(), a.raw.map(|i| i.leaf)),
        (Some("s"), Some(2.0))
    );
    let r: ReadsRaw = toml::from_str("first = \"f\"\nraw = 1").unwrap();
    assert_eq!((r.first.as_deref(), r.raw.is_some()), (Some("f"), true));
    let bare = Either::deserialize(toml::Value::String("b".into())).unwrap();
    assert!(matches!(bare, Either::Bare(ref s) if s == "b"));
}
