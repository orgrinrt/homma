//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The wrapper as a script that runs: whether it hands off is a property of
//! bash, and bash is available, so these run what homma writes rather than
//! reading its text.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::cmd::aggregate::tests::make_executable;

/// Run an emitted wrapper with a tool-input payload naming `target`.
fn run_wrapper(wrapper: &Path, target: &Path) {
    run_wrapper_output(wrapper, target);
}

/// As `run_wrapper`, keeping what the process said and how it exited.
fn run_wrapper_output(wrapper: &Path, target: &Path) -> std::process::Output {
    run_wrapper_on_path(wrapper, target, None)
}

/// The same, with `path` replacing `PATH` for the child.
///
/// A machine without `jq` is reproduced on one that has it by handing over
/// a directory holding only `bash`.
fn run_wrapper_on_path(wrapper: &Path, target: &Path, path: Option<&str>) -> std::process::Output {
    let payload = format!(r#"{{"tool_input":{{"file_path":"{}"}}}}"#, target.display());
    run_payload(wrapper, &payload, &[("PATH", path)])
}

/// Run a wrapper with `payload` on stdin, each `(name, Some(value))` set in its
/// environment.
fn run_payload(
    wrapper: &Path,
    payload: &str,
    env: &[(&str, Option<&str>)],
) -> std::process::Output {
    use std::io::Write;
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(wrapper)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in env {
        if let Some(v) = v {
            cmd.env(k, v);
        }
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// A workspace holding repository `rel` with a hook `foo.sh` that records it
/// ran, and the wrapper for it. Returns the wrapper and the marker.
fn planted(ws: &Path, rel: &str) -> (PathBuf, PathBuf) {
    let hooks = ws.join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::create_dir_all(ws.join(rel).join(".claude/hooks")).unwrap();
    let marker = ws.join("fired");
    let real = ws.join(rel).join(".claude/hooks/foo.sh");
    fs::write(
        &real,
        format!(
            "#!/usr/bin/env bash\ncat > /dev/null\ntouch '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    make_executable(&real);
    let wrapper = hooks.join("arvo--foo.sh");
    fs::write(
        &wrapper,
        wrapper_script("arvo", rel, ".claude/hooks/foo.sh", ""),
    )
    .unwrap();
    make_executable(&wrapper);
    (wrapper, marker)
}

/// Whether a shell call running `command` in `cwd` reaches the hook.
fn shell_call_fires(wrapper: &Path, marker: &Path, cwd: &Path, command: &str) -> bool {
    let _ = fs::remove_file(marker);
    let payload = serde_json::json!({
        "cwd": cwd.display().to_string(),
        "tool_input": { "command": command },
    })
    .to_string();
    let out = run_payload(wrapper, &payload, &[]);
    assert!(
        out.status.success(),
        "the wrapper failed on `{command}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    marker.exists()
}

#[test]
fn a_wrapper_carries_no_absolute_path_and_finds_its_workspace_from_its_own_location() {
    let s = wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh", "");
    assert!(s.starts_with("#!/usr/bin/env bash"));
    assert!(s.contains("REPO_REL='arvo'"));
    assert!(s.contains("ORIG_HOOK=\"$REPO_ROOT\"'/.claude/hooks/foo.sh'"));
    assert!(s.contains("BASH_SOURCE[0]"));
    assert!(s.contains("$ORIG_HOOK"));
    assert!(s.contains("Aggregated from `arvo`"));

    // The whole point, and the assertion the old shape could not have
    // passed: nothing in the emitted script names a machine. A path
    // starting at the filesystem root is a fact about the workspace that
    // generated the file and is inert in every other clone.
    for line in s.lines() {
        let code = line.split('#').next().unwrap_or(line);
        assert!(
            !code.contains("='/") && !code.contains("=\"/"),
            "wrapper assigns an absolute path: {line}"
        );
    }
}

#[test]
fn a_wrapper_matches_a_path_under_its_repo_and_declines_one_outside() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");

    run_wrapper(&wrapper, &ws.path().join("arvo/src/lib.rs"));
    assert!(
        marker.exists(),
        "the wrapper did not hand off for a path inside the repo"
    );

    // The control. Without it, a wrapper that handed off unconditionally
    // would pass the assertion above and be exactly the guard-shaped thing
    // that guards nothing.
    fs::remove_file(&marker).unwrap();
    run_wrapper(&wrapper, &ws.path().join("kolli/src/lib.rs"));
    assert!(
        !marker.exists(),
        "the wrapper handed off for a path outside the repo"
    );
}

/// **Without `jq` the wrapper hands off rather than guessing.**
///
/// A write inside the repo from anywhere else must not skip the repo's own
/// hook, and handing off is the safe direction: it is what happens with no
/// aggregation at all, so the cost of being wrong is a hook running when it
/// need not rather than one that never runs.
#[test]
fn a_wrapper_without_jq_hands_off_rather_than_guessing_from_the_directory() {
    let ws = tempfile::tempdir().unwrap();
    let hooks = ws.path().join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::create_dir_all(ws.path().join("arvo/.claude/hooks")).unwrap();

    // Builtins only, because this runs under a PATH holding one program.
    let marker = ws.path().join("fired");
    let real = ws.path().join("arvo/.claude/hooks/foo.sh");
    fs::write(
        &real,
        format!("#!/usr/bin/env bash\nprintf '' > '{}'\n", marker.display()),
    )
    .unwrap();
    make_executable(&real);

    let wrapper = hooks.join("arvo--foo.sh");
    fs::write(
        &wrapper,
        wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh", ""),
    )
    .unwrap();
    make_executable(&wrapper);

    let bare = crate::cmd::gates::tests::a_path_without_jq(ws.path());

    let inside = ws.path().join("arvo/src/lib.rs");
    run_wrapper_on_path(&wrapper, &inside, Some(&bare));
    assert!(
        marker.exists(),
        "a write inside the repo skipped the repo's own hook when jq was absent"
    );

    // The control: with jq present a path outside is declined, so the
    // assertion above is about the missing tool.
    fs::remove_file(&marker).unwrap();
    run_wrapper(&wrapper, &ws.path().join("kolli/src/lib.rs"));
    assert!(
        !marker.exists(),
        "control: with jq present a path outside the repo is still declined"
    );
}

#[test]
fn a_wrapper_declines_when_the_repo_is_not_cloned_here() {
    // The target has to be INSIDE the absent repo. A path outside it exits 0
    // through the scope check whether or not the presence check exists, so a
    // test aiming there measures nothing.
    let ws = tempfile::tempdir().unwrap();
    let hooks = ws.path().join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();

    for runner in ["", "bash"] {
        let wrapper = hooks.join("arvo--foo.sh");
        fs::write(
            &wrapper,
            wrapper_script("arvo", "arvo", ".claude/hooks/foo.sh", runner),
        )
        .unwrap();
        make_executable(&wrapper);

        let out = run_wrapper_output(&wrapper, &ws.path().join("arvo/src/lib.rs"));
        assert!(
            out.status.success(),
            "runner `{runner}`: a wrapper with nothing to run failed instead of declining: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "runner `{runner}`: a wrapper with nothing to run said something: out={} err={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn a_relative_repo_root_never_matches_the_absolute_path_the_host_supplies() {
    // The host always supplies an absolute `file_path`, and the comparison is
    // textual.
    let matches = |root: &str, target: &str| -> bool {
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                r#"case "{target}" in "{root}"|"{root}"/*) exit 0;; *) exit 1;; esac"#
            ))
            .output()
            .unwrap();
        out.status.success()
    };
    assert!(!matches("./arvo", "/ws/arvo/src/lib.rs"));
    assert!(matches("./arvo", "./arvo/src/lib.rs"));
    assert!(matches("/ws/arvo", "/ws/arvo/src/lib.rs"));
}

#[test]
fn a_shell_call_at_the_root_reaches_the_hook_of_every_repo_it_names_by_path() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
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
            shell_call_fires(&wrapper, &marker, root, command),
            "`{command}` at the root did not reach arvo's hook"
        );
    }
    let absolute = format!("cargo test --manifest-path {}", abs.display());
    assert!(
        shell_call_fires(&wrapper, &marker, root, &absolute),
        "{absolute}"
    );

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
            !shell_call_fires(&wrapper, &marker, root, command),
            "`{command}` at the root reached arvo's hook"
        );
    }
}

#[test]
fn a_shell_call_inside_the_repo_reaches_it_whatever_its_command_names() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
    let inside = ws.path().join("arvo/src");
    assert!(shell_call_fires(&wrapper, &marker, &inside, "cargo test"));
    assert!(shell_call_fires(&wrapper, &marker, &inside, "ls /tmp"));
    // The control: the same call elsewhere does not.
    assert!(!shell_call_fires(
        &wrapper,
        &marker,
        &ws.path().join("kolli"),
        "cargo test"
    ));
}

#[test]
fn a_climb_out_of_the_directory_lands_everywhere() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
    let kolli = ws.path().join("kolli");
    assert!(shell_call_fires(
        &wrapper,
        &marker,
        &kolli,
        "cargo test --manifest-path ../x/Cargo.toml"
    ));
    assert!(shell_call_fires(&wrapper, &marker, &kolli, "cd .. && ls"));
    // The control: dots that are not a climb.
    assert!(!shell_call_fires(&wrapper, &marker, &kolli, "cat a..b ..x"));
}

#[test]
fn a_path_under_home_is_read_off_its_tilde() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
    let home = ws.path().to_str().unwrap();
    let call = |command: &str| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({
            "cwd": "/",
            "tool_input": { "command": command },
        })
        .to_string();
        run_payload(&wrapper, &payload, &[("HOME", Some(home))]);
        marker.exists()
    };
    assert!(call("cargo build --manifest-path ~/arvo/Cargo.toml"));
    assert!(!call("cargo build --manifest-path ~/kolli/Cargo.toml"));
}

#[test]
fn an_absolute_path_with_a_space_in_it_is_found_whole() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "a tree");
    let root = ws.path();
    let abs = ws.path().join("a tree/Cargo.toml");
    let command = format!("cargo build --manifest-path '{}'", abs.display());
    assert!(
        shell_call_fires(&wrapper, &marker, root, &command),
        "{command}"
    );
    let other = format!(
        "cargo build --manifest-path '{}'",
        ws.path().join("a treehouse/x").display()
    );
    assert!(
        !shell_call_fires(&wrapper, &marker, root, &other),
        "{other}"
    );
}

/// **Stated in the design as not seen.** A relative path with a space in it is
/// read as its pieces, and neither piece is the repository. The test pins the
/// stated limit, so reading it after all is a change somebody notices.
#[test]
fn a_relative_path_with_a_space_in_it_is_the_stated_blind_spot() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "a tree");
    assert!(!shell_call_fires(
        &wrapper,
        &marker,
        ws.path(),
        "cd 'a tree' && cargo build"
    ));
}

#[test]
fn a_file_tool_is_narrowed_by_its_file_and_a_relative_one_is_read_from_the_directory() {
    let ws = tempfile::tempdir().unwrap();
    let (wrapper, marker) = planted(ws.path(), "arvo");
    let call = |cwd: &Path, file: &str| {
        let _ = fs::remove_file(&marker);
        let payload = serde_json::json!({
            "cwd": cwd.display().to_string(),
            "tool_input": { "file_path": file, "command": "arvo" },
        })
        .to_string();
        run_payload(&wrapper, &payload, &[]);
        marker.exists()
    };
    assert!(call(ws.path(), "arvo/src/lib.rs"));
    // The file decides, whatever the session's directory and whatever else
    // the payload carries.
    let outside = ws.path().join("kolli/x.rs");
    assert!(!call(&ws.path().join("arvo"), outside.to_str().unwrap()));
}

#[test]
fn a_hook_run_through_a_program_is_run_through_it() {
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(ws.path().join(".claude/hooks")).unwrap();
    fs::create_dir_all(ws.path().join("arvo/.claude/hooks")).unwrap();
    let marker = ws.path().join("fired");
    // Not executable, and a shape only bash runs, so whether it ran through
    // the program is what the marker says.
    let real = ws.path().join("arvo/.claude/hooks/check.sh");
    fs::write(
        &real,
        format!(
            "cat > /dev/null\nprintf '%s' \"$*\" > '{}'\n",
            marker.display()
        ),
    )
    .unwrap();
    let wrapper = ws.path().join(".claude/hooks/arvo--check.sh");
    fs::write(
        &wrapper,
        wrapper_script("arvo", "arvo", ".claude/hooks/check.sh", "bash"),
    )
    .unwrap();
    make_executable(&wrapper);

    let payload = format!(
        r#"{{"tool_input":{{"file_path":"{}"}}}}"#,
        ws.path().join("arvo/x").display()
    );
    use std::io::Write;
    let mut child = std::process::Command::new("bash")
        .arg(&wrapper)
        .arg("--strict")
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "--strict");

    // The control: the same file with no program is not run, since it is not
    // executable and the wrapper declines.
    fs::remove_file(&marker).unwrap();
    fs::write(
        &wrapper,
        wrapper_script("arvo", "arvo", ".claude/hooks/check.sh", ""),
    )
    .unwrap();
    make_executable(&wrapper);
    let out = run_payload(&wrapper, &payload, &[]);
    assert!(out.status.success());
    assert!(
        !marker.exists(),
        "a file that is not executable ran with no program"
    );
}
