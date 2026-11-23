//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The dispatcher's own timer leaves nothing running behind it: not what a hook
//! stopped for its time had started, and not the timer of a hook done in time.

use std::fs;

use super::tests::{dispatch, entry, hook, plant, text};

/// Whether process `pid` is still there, given up to two seconds to go.
fn still_running(pid: &str) -> bool {
    for _ in 0 .. 20 {
        let alive = std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    true
}

/// The processes whose whole command line is `line`.
fn running(line: &str) -> String {
    let out = std::process::Command::new("pgrep")
        .args(["-fx", line])
        .output()
        .unwrap();
    text(&out.stdout)
}

#[test]
fn a_hook_stopped_for_its_time_takes_what_it_started_with_it() {
    let ws = tempfile::tempdir().unwrap();
    let pid = ws.path().join("child");
    hook(
        ws.path(),
        "h/slow",
        &format!(
            "cat > /dev/null; sleep 30 & echo $! > '{}'; wait",
            pid.display()
        ),
    );
    let mut slow = entry("PreToolUse", "*", "h/slow", &[]);
    slow.timeout = 1;
    let d = plant(ws.path(), &[slow], &[]);
    dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
    let child = fs::read_to_string(&pid).unwrap().trim().to_string();
    assert!(
        !still_running(&child),
        "the hook's child {child} outlived the hook it was stopped with"
    );
}

#[test]
fn a_hook_done_in_time_leaves_no_timer_behind() {
    // the control: pgrep finds a sleep of that line while one runs
    let mut probe = std::process::Command::new("sleep")
        .arg("171")
        .spawn()
        .unwrap();
    let seen = running("sleep 171");
    probe.kill().unwrap();
    probe.wait().unwrap();
    assert!(!seen.is_empty(), "pgrep found no sleep that was running");

    let ws = tempfile::tempdir().unwrap();
    let mut entries = Vec::new();
    for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        hook(ws.path(), &format!("h/{name}"), "cat > /dev/null");
        let mut e = entry("PreToolUse", "*", &format!("h/{name}"), &[]);
        // a time no other test gives, so its sleep is this test's alone
        e.timeout = 173;
        entries.push(e);
    }
    let d = plant(ws.path(), &entries, &[]);
    for _ in 0 .. 10 {
        let out = dispatch(&d, &["PreToolUse", "sync"], "{}", None, &[]);
        assert!(out.status.success(), "{}", text(&out.stderr));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    let left = running("sleep 173");
    assert!(left.is_empty(), "timers outlived their hooks: {left}");
}
