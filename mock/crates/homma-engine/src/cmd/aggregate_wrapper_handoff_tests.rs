//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a hook is handed once the wrapper decides it runs: where it runs, and
//! how a file path that climbs is read. These run the emitted bash, and the hook
//! records what it saw rather than only that it ran, since a hook that only
//! touches a marker cannot tell a test where it was.

use std::fs;
use std::path::Path;

use super::tests::{planted, run_payload};
use super::*;
use crate::cmd::aggregate::tests::make_executable;

/// Rewrite the planted hook to record the directory it runs in.
fn records_its_directory(ws: &Path, rel: &str, marker: &Path) {
    let real = ws.join(rel).join(".claude/hooks/foo.sh");
    fs::write(
        &real,
        format!(
            "#!/usr/bin/env bash\ncat > /dev/null\npwd -P > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    make_executable(&real);
}

/// Run the wrapper as the host does, in `cwd`, with a shell call there.
fn shell_call_in(wrapper: &Path, cwd: &Path, command: &str) {
    use std::io::Write;
    let payload = serde_json::json!({
        "cwd": cwd.display().to_string(),
        "tool_input": { "command": command },
    })
    .to_string();
    let mut child = std::process::Command::new("bash")
        .arg(wrapper)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

/// The hook runs where the session is, at the root and inside the repository
/// alike, and not at the repository's root. At the root is the case a guard
/// scoping itself by its own directory declines, and the design says so rather
/// than the wrapper moving it.
#[test]
fn a_hook_runs_in_the_directory_the_host_ran_it_in() {
    let ws = tempfile::tempdir().unwrap();
    let ws = ws.path().canonicalize().unwrap();
    let (wrapper, marker) = planted(&ws, "arvo");
    records_its_directory(&ws, "arvo", &marker);
    let inside = ws.join("arvo/src");
    fs::create_dir_all(&inside).unwrap();

    shell_call_in(&wrapper, &ws, "cp a arvo/src/x");
    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        ws.display().to_string(),
        "a call typed at the root ran its hook somewhere else"
    );

    // The control: the recorded directory moves with the session, so the arm
    // above is reading where the hook ran rather than a constant.
    fs::remove_file(&marker).unwrap();
    shell_call_in(&wrapper, &inside, "cp a x");
    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        inside.display().to_string()
    );
}

/// A file path that climbs lands everywhere, as a command's piece does, so one
/// reaching the repository through a sibling is not missed.
#[test]
fn a_file_path_that_climbs_lands_everywhere() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
    let fires = |path: String| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({ "tool_input": { "file_path": path } }).to_string();
        assert!(run_payload(&wrapper, &payload, &[]).status.success());
        marker.exists()
    };
    let ws = ws.path().display();
    assert!(fires(format!("{ws}/other/../arvo/src/lib.rs")));
    assert!(fires(format!("{ws}/other/..")));
    // The control: the same sibling without the climb is outside.
    assert!(!fires(format!("{ws}/other/src/lib.rs")));
    // And a name that only contains two dots is not a climb.
    assert!(!fires(format!("{ws}/other/a..b")));
}

/// A runner is refused where it is a line of shell rather than a program, and
/// kept where it is a program, arguments and all.
#[test]
fn a_runner_that_is_a_line_of_shell_is_refused() {
    let one = |r: &str| one_runner(std::iter::once(r), "arvo/.claude/hooks/x.sh");
    for shell in [
        "FOO=1",
        "cd dir &&",
        "true;",
        "cat |",
        "(cd dir;",
        "`which bash`",
        "$(which bash)",
        "bash <",
    ] {
        let got = one(shell);
        assert!(
            got.as_ref().is_err_and(|e| e.contains("a line of shell")),
            "`{shell}` was kept as a runner: {got:?}"
        );
    }
    // The controls: a program, one with arguments carrying an `=`, and none.
    assert_eq!(one("bash"), Ok("bash".to_string()));
    assert_eq!(one("env A=1 node"), Ok("env A=1 node".to_string()));
    assert_eq!(one("node --flag=x"), Ok("node --flag=x".to_string()));
    assert_eq!(one(""), Ok(String::new()));
    // Two ways is refused for its own reason, and says which.
    let two = one_runner(["", "bash"].into_iter(), "arvo/.claude/hooks/x.sh");
    assert!(two.is_err_and(|e| e.contains("itself and `bash`")));
}
