//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::fs;

use super::*;

fn row(event: &str, name: &str) -> Row {
    Row {
        event:    event.into(),
        name:     name.into(),
        run:      vec!["{root}/x".into()],
        matcher:  String::new(),
        paths:    Vec::new(),
        at_root:  false,
        timeout:  DEFAULT_TIMEOUT,
        filtered: false,
    }
}

fn parsed(text: &str) -> toml::Table {
    text.parse().expect("the manifest parses")
}

#[test]
fn the_manifest_keeps_each_events_rows_in_order_with_their_keys() {
    let mut a = row("PreToolUse", "b");
    a.matcher = "Bash".into();
    a.paths = vec!["engine".into()];
    a.timeout = 30;
    let mut c = row("SessionStart", "c");
    c.at_root = true;
    let t = parsed(&manifest(&[a, row("PreToolUse", "a"), c], false));
    assert_eq!(t["name"].as_str(), Some("homma"));
    let pre = t["PreToolUse"].as_array().unwrap();
    let names: Vec<&str> = pre.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["b", "a"]);
    assert_eq!(pre[0]["matcher"].as_str(), Some("Bash"));
    assert_eq!(
        pre[0]["paths"].as_array().unwrap()[0].as_str(),
        Some("engine")
    );
    assert_eq!(pre[0]["timeout"].as_integer(), Some(30));
    assert!(
        pre[1].get("matcher").is_none()
            && pre[1].get("paths").is_none()
            && pre[1].get("dir").is_none()
    );
    assert_eq!(
        t["SessionStart"].as_array().unwrap()[0]["dir"].as_str(),
        Some(".")
    );
}

#[test]
fn a_filtered_row_runs_through_the_filter_only_where_the_workspace_has_it() {
    let mut r = row("PreToolUse", "a");
    r.filtered = true;
    let with = parsed(&manifest(&[r.clone(), row("PreToolUse", "b")], true));
    let pre = with["PreToolUse"].as_array().unwrap();
    let run = |i: usize| -> Vec<String> {
        pre[i]["run"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(run(0), [
        "{root}/tools/hookdedup/hookdedup",
        "run",
        "{root}/x"
    ]);
    assert_eq!(run(1), ["{root}/x"]);
    let without = parsed(&manifest(&[r], false));
    assert_eq!(
        without["PreToolUse"].as_array().unwrap()[0]["run"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_second_row_under_one_name_at_one_event_is_numbered() {
    let rows = numbered(vec![
        row("Stop", "a"),
        row("Stop", "a"),
        row("PreToolUse", "a"),
        row("Stop", "a"),
    ]);
    let names: Vec<(&str, &str)> = rows
        .iter()
        .map(|r| (r.event.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(names, [
        ("Stop", "a"),
        ("Stop", "a-2"),
        ("PreToolUse", "a"),
        ("Stop", "a-3")
    ]);
    assert_eq!(row_name("acre--check message.sh"), "acre--check_message.sh");
}

fn front_saying(dir: &std::path::Path, status: i32) -> std::path::PathBuf {
    let f = dir.join("front");
    fs::write(&f, format!("#!/bin/sh\nprintf '%s ' \"$@\" > \"$(dirname \"$0\")/args\"\necho refused here >&2\nexit {status}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&f, fs::Permissions::from_mode(0o755)).unwrap();
    }
    f
}

#[test]
fn an_install_writes_the_manifest_and_has_the_front_register_it_at_the_root() {
    let ws = tempfile::tempdir().unwrap();
    let root = crate::cmd::aggregate::tests::test_root(ws.path());
    let bin = tempfile::tempdir().unwrap();
    let front = front_saying(bin.path(), 0);
    install(&root, &front, &[row("Stop", "a")]).unwrap();
    let text = fs::read_to_string(ws.path().join(MANIFEST)).unwrap();
    assert!(parsed(&text).contains_key("Stop"));
    let args = fs::read_to_string(bin.path().join("args")).unwrap();
    assert_eq!(
        args.trim(),
        format!("install --root {}", root.as_abs().display())
    );
}

#[test]
fn a_refused_install_puts_the_manifest_back_and_says_why() {
    let ws = tempfile::tempdir().unwrap();
    let root = crate::cmd::aggregate::tests::test_root(ws.path());
    let bin = tempfile::tempdir().unwrap();
    install(&root, &front_saying(bin.path(), 0), &[row(
        "Stop", "before",
    )])
    .unwrap();
    let before = fs::read_to_string(ws.path().join(MANIFEST)).unwrap();
    let err = install(&root, &front_saying(bin.path(), 1), &[row("Stop", "after")]).unwrap_err();
    assert!(format!("{err:#}").contains("refused here"), "{err:#}");
    assert_eq!(
        fs::read_to_string(ws.path().join(MANIFEST)).unwrap(),
        before
    );
    // and where there was none, there is none again
    let ws2 = tempfile::tempdir().unwrap();
    let root2 = crate::cmd::aggregate::tests::test_root(ws2.path());
    assert!(install(&root2, &front_saying(bin.path(), 1), &[row("Stop", "a")]).is_err());
    assert!(!ws2.path().join(MANIFEST).exists());
}
