//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where a call lands, and so which of the repositories' hooks it reaches, and
//! what a hook is handed once the dispatcher decides it runs. These run the
//! emitted bash, and a hook records what it saw where where matters.

use std::fs;
use std::path::{Path, PathBuf};

use super::tests::{dispatch, entry, hook, plant};

/// A workspace holding repository `rel` with a hook `foo.sh` that records it
/// ran, and the dispatcher with its one entry, narrowed to `rel`. Returns the
/// dispatcher and the marker.
fn planted(ws: &Path, rel: &str) -> (PathBuf, PathBuf) {
    let marker = ws.join("fired");
    let run = format!("{rel}/.claude/hooks/foo.sh");
    hook(
        ws,
        &run,
        &format!("cat > /dev/null\ntouch '{}'", marker.display()),
    );
    let d = plant(ws, &[entry("PreToolUse", "*", &run, &[rel])], &[]);
    (d, marker)
}

/// Run the dispatcher at `PreToolUse` with `payload`.
fn run_payload(d: &Path, payload: &str, env: &[(&str, Option<&str>)]) -> std::process::Output {
    dispatch(d, &["PreToolUse"], payload, None, env)
}

/// Whether a file tool on `target` reaches the hook.
fn file_call_fires(d: &Path, marker: &Path, target: &Path) -> bool {
    let _ = fs::remove_file(marker);
    let payload =
        serde_json::json!({ "tool_input": { "file_path": target.display().to_string() } })
            .to_string();
    let out = run_payload(d, &payload, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    marker.exists()
}

/// Whether a shell call running `command` in `cwd` reaches the hook.
fn shell_call_fires(d: &Path, marker: &Path, cwd: &Path, command: &str) -> bool {
    let _ = fs::remove_file(marker);
    let payload = serde_json::json!({
        "cwd": cwd.display().to_string(),
        "tool_input": { "command": command },
    })
    .to_string();
    let out = run_payload(d, &payload, &[]);
    assert!(
        out.status.success(),
        "the dispatcher failed on `{command}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    marker.exists()
}

#[test]
fn a_path_under_the_repo_reaches_its_hook_and_one_outside_does_not() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    assert!(
        file_call_fires(&d, &marker, &ws.path().join("arvo/src/lib.rs")),
        "a path inside the repo did not reach its hook"
    );
    // The control. Without it, a dispatcher that ran every hook would pass the
    // assertion above and be exactly the guard-shaped thing that guards nothing.
    assert!(
        !file_call_fires(&d, &marker, &ws.path().join("kolli/src/lib.rs")),
        "a path outside the repo reached its hook"
    );
}

#[test]
fn an_entry_narrowed_to_several_repos_takes_a_call_in_any_and_none_elsewhere() {
    let ws = tempfile::tempdir().unwrap();
    let marker = ws.path().join("fired");
    hook(
        ws.path(),
        "h/x",
        &format!("cat > /dev/null; touch '{}'", marker.display()),
    );
    let d = plant(
        ws.path(),
        &[entry("PreToolUse", "*", "h/x", &["arvo", "kolli"])],
        &[],
    );
    assert!(file_call_fires(&d, &marker, &ws.path().join("arvo/a")));
    assert!(file_call_fires(&d, &marker, &ws.path().join("kolli/a")));
    assert!(!file_call_fires(&d, &marker, &ws.path().join("loru/a")));
}

#[test]
fn a_shell_call_at_the_root_reaches_the_hook_of_every_repo_it_names_by_path() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let root = ws.path();
    let abs = ws.path().join("arvo/mock/Cargo.toml");

    for command in [
        "cargo test --manifest-path arvo/mock/Cargo.toml",
        "cargo test --manifest-path=arvo/mock/Cargo.toml",
        "git -C arvo commit -m x",
        "cd arvo && cargo build",
        "cd 'arvo' && cargo build",
        "(cd arvo; cargo bench)",
        "MOCK_ROOT=arvo/mock cargo mock lock",
        "cargo build > arvo/log.txt",
        "ls ./arvo/src",
        "ls ../ws/arvo",
    ] {
        assert!(
            shell_call_fires(&d, &marker, root, command),
            "`{command}` at the root did not reach arvo's hook"
        );
    }
    let absolute = format!("cargo test --manifest-path {}", abs.display());
    assert!(shell_call_fires(&d, &marker, root, &absolute), "{absolute}");

    // The controls, from the same directory: a call naming no path in the
    // repository, and ones naming a neighbour whose name only starts the same.
    for command in [
        "cargo test -p kolli",
        "git -C kolli commit -m x",
        "cat arvo-notes.md",
        "ls arvo2/src",
        "git status",
        "",
    ] {
        assert!(
            !shell_call_fires(&d, &marker, root, command),
            "`{command}` at the root reached arvo's hook"
        );
    }
}

#[test]
fn a_shell_call_inside_the_repo_reaches_it_whatever_its_command_names() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let inside = ws.path().join("arvo/src");
    assert!(shell_call_fires(&d, &marker, &inside, "cargo test"));
    assert!(shell_call_fires(&d, &marker, &inside, "ls /tmp"));
    // The control: the same call elsewhere does not.
    assert!(!shell_call_fires(
        &d,
        &marker,
        &ws.path().join("kolli"),
        "cargo test"
    ));
}

#[test]
fn a_climb_out_of_the_directory_lands_everywhere() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let kolli = ws.path().join("kolli");
    assert!(shell_call_fires(
        &d,
        &marker,
        &kolli,
        "cargo test --manifest-path ../x/Cargo.toml"
    ));
    assert!(shell_call_fires(&d, &marker, &kolli, "cd .. && ls"));
    // The control: dots that are not a climb.
    assert!(!shell_call_fires(&d, &marker, &kolli, "cat a..b ..x"));
}

#[test]
fn a_path_under_home_is_read_off_its_tilde() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let home = ws.path().to_str().unwrap();
    let call = |command: &str| {
        let _ = fs::remove_file(&marker);
        let payload =
            serde_json::json!({ "cwd": "/", "tool_input": { "command": command } }).to_string();
        run_payload(&d, &payload, &[("HOME", Some(home))]);
        marker.exists()
    };
    assert!(call("cargo build --manifest-path ~/arvo/Cargo.toml"));
    assert!(!call("cargo build --manifest-path ~/kolli/Cargo.toml"));
}

#[test]
fn an_absolute_path_with_a_space_in_it_is_found_whole() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "a tree");
    let root = ws.path();
    let abs = ws.path().join("a tree/Cargo.toml");
    let command = format!("cargo build --manifest-path '{}'", abs.display());
    assert!(shell_call_fires(&d, &marker, root, &command), "{command}");
    let other = format!(
        "cargo build --manifest-path '{}'",
        ws.path().join("a treehouse/x").display()
    );
    assert!(!shell_call_fires(&d, &marker, root, &other), "{other}");
}

/// **Stated in the design as not seen.** A relative path with a space in it is
/// read as its pieces, and neither piece is the repository. The test pins the
/// stated limit, so reading it after all is a change somebody notices.
#[test]
fn a_relative_path_with_a_space_in_it_is_the_stated_blind_spot() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "a tree");
    assert!(!shell_call_fires(
        &d,
        &marker,
        ws.path(),
        "cd 'a tree' && cargo build"
    ));
}

#[test]
fn a_file_tool_is_narrowed_by_its_file_and_a_relative_one_is_read_from_the_directory() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let call = |cwd: &Path, file: &str| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({
            "cwd": cwd.display().to_string(),
            "tool_input": { "file_path": file, "command": "arvo" },
        })
        .to_string();
        run_payload(&d, &payload, &[]);
        marker.exists()
    };
    assert!(call(ws.path(), "arvo/src/lib.rs"));
    // The file decides, whatever the session's directory and whatever else
    // the payload carries.
    let outside = ws.path().join("kolli/x.rs");
    assert!(!call(&ws.path().join("arvo"), outside.to_str().unwrap()));
}

/// A file path that climbs lands everywhere, as a command's piece does, so one
/// reaching the repository through a sibling is not missed.
#[test]
fn a_file_path_that_climbs_lands_everywhere() {
    let ws = tempfile::tempdir().unwrap();
    let (d, marker) = planted(ws.path(), "arvo");
    let w = ws.path().display();
    assert!(file_call_fires(
        &d,
        &marker,
        Path::new(&format!("{w}/other/../arvo/src/lib.rs"))
    ));
    assert!(file_call_fires(
        &d,
        &marker,
        Path::new(&format!("{w}/other/.."))
    ));
    // The control: the same sibling without the climb is outside.
    assert!(!file_call_fires(
        &d,
        &marker,
        Path::new(&format!("{w}/other/src/lib.rs"))
    ));
    // And a name that only contains two dots is not a climb.
    assert!(!file_call_fires(
        &d,
        &marker,
        Path::new(&format!("{w}/other/a..b"))
    ));
}

/// The hook runs where the session is, at the root and inside the repository
/// alike, and not at the repository's root. At the root is the case a guard
/// scoping itself by its own directory declines, and the design says so rather
/// than the dispatcher moving it.
#[test]
fn a_hook_runs_in_the_directory_the_host_ran_it_in() {
    let ws = tempfile::tempdir().unwrap();
    let ws = ws.path().canonicalize().unwrap();
    let marker = ws.join("where");
    let run = "arvo/.claude/hooks/foo.sh";
    hook(
        &ws,
        run,
        &format!("cat > /dev/null\npwd -P > '{}'", marker.display()),
    );
    let d = plant(&ws, &[entry("PreToolUse", "*", run, &["arvo"])], &[]);
    let inside = ws.join("arvo/src");
    fs::create_dir_all(&inside).unwrap();
    let shell_call_in = |cwd: &Path, command: &str| {
        let payload = serde_json::json!({
            "cwd": cwd.display().to_string(),
            "tool_input": { "command": command },
        })
        .to_string();
        assert!(
            dispatch(&d, &["PreToolUse"], &payload, Some(cwd), &[])
                .status
                .success()
        );
    };

    shell_call_in(&ws, "cp a arvo/src/x");
    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        ws.display().to_string(),
        "a call typed at the root ran its hook somewhere else"
    );
    // The control: the recorded directory moves with the session, so the arm
    // above is reading where the hook ran rather than a constant.
    fs::remove_file(&marker).unwrap();
    shell_call_in(&inside, "cp a x");
    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        inside.display().to_string()
    );
}

/// The hook is handed the payload exactly as the host sent it.
#[test]
fn a_hook_is_handed_the_payload_the_host_sent() {
    let ws = tempfile::tempdir().unwrap();
    let got = ws.path().join("got");
    hook(ws.path(), "h/x", &format!("cat > '{}'", got.display()));
    let d = plant(
        ws.path(),
        &[entry("Stop", "*", "h/x", &[]), entry("PreToolUse", "*", "h/x", &[])],
        &[],
    );
    let payload = r#"{"session_id":"s","stop_hook_active":false}"#;
    // Alone and together alike, with the one newline the hand-off adds after it,
    // which a JSON reader passes over.
    for args in [&["Stop", "one", "0"][..], &["PreToolUse", "sync"][..]] {
        let _ = fs::remove_file(&got);
        assert!(dispatch(&d, args, payload, None, &[]).status.success());
        assert_eq!(fs::read_to_string(&got).unwrap(), format!("{payload}\n"));
    }
}

/// A runner is refused where it is a line of shell rather than a program, and
/// kept where it is a program, arguments and all.
#[test]
fn a_runner_that_is_a_line_of_shell_is_refused() {
    use crate::cmd::aggregate::one_runner;
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
