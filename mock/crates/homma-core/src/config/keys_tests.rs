//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use crate::config::{Config, keys};

fn listed() -> Vec<String> {
    keys().read().iter().cloned().collect()
}

#[test]
fn the_keys_of_both_parsers_are_listed_and_a_raw_table_is_not() {
    let listed = listed();
    for key in [
        // read by the core's own types
        "workspace.name",
        "session.cloud_var",
        "auth.service_prefix",
        "forges.*.kind",
        // read by the api's types from the tables the core holds raw
        "paths.captures",
        "org.*.role",
        "lease.branch",
        "search.members",
        "summary.model",
        "ask.cache_expire_hours",
    ] {
        assert!(
            listed.iter().any(|k| k == key),
            "{key} is not listed: {listed:?}"
        );
    }
    // Failing control: a table held raw is listed only by the keys under it.
    for raw in ["paths", "org", "lease", "search", "summary", "ask"] {
        assert!(!listed.iter().any(|k| k == raw), "{raw} is listed whole");
    }
}

#[test]
fn the_keys_left_opaque_are_the_named_values_of_two_spellings() {
    // Each is read as any value because it takes two spellings: `deny` an entry that is a path or
    // a table of `path` and `why`, `status.inject.tool` a program or an argument list. Neither
    // refuses an unknown key under it in any engine. A new table held as a raw value, or another
    // type that reads any value, joins this set and fails here until the type that reads it is
    // grafted in or it is named here. Failing control: take one graft out of `keys` and its
    // table's name is here.
    let keys = keys();
    let opaque: Vec<&str> = keys.opaque().iter().map(String::as_str).collect();
    assert_eq!(opaque, ["deny", "status.inject.tool"]);
}

#[test]
fn every_listed_key_of_a_plain_value_is_one_this_engine_reads() {
    // Failing control: a key the engine does not read is refused at parse.
    let bogus = "[workspace]\nname = \"w\"\n[session]\nnot_a_key = \"x\"\n";
    assert!(Config::parse(bogus).is_err());
    for key in ["session.cloud_var", "auth.service_prefix"] {
        assert!(listed().iter().any(|k| k == key), "{key}");
        let (table, field) = key.split_once('.').unwrap();
        let text = format!("[workspace]\nname = \"w\"\n[{table}]\n{field} = \"x\"\n");
        assert!(Config::parse(&text).is_ok(), "{key} is listed and not read");
    }
}
