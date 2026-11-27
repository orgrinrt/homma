//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma agent regen` moves a clone's agent hooks onto claude-shook only
//! where the front takes them, and sweeps what an older homma registered only
//! after it has.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MARK: &str = "# written by `homma agent regen`";

/// What an older homma left: the wrapper, its registration, and beside it a
/// hook somebody wrote and what the person at the clone allowed.
const OLD_SETTINGS: &str = r#"{"permissions":{"allow":["Artifact"]},"hooks":{"PreToolUse":[
    {"matcher":"Edit","hooks":[{"type":"command","command":"\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/acre--guard.sh"}]},
    {"matcher":"Bash","hooks":[{"type":"command","command":".claude/hooks/mine.sh"}]}
]}}"#;

struct Ws {
    _dir: tempfile::TempDir,
    root: PathBuf,
    cfg:  PathBuf,
    home: PathBuf,
}

fn git_init(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["init", "-q"])
        .output()
        .unwrap();
    assert!(out.status.success(), "git init: {out:?}");
}

/// A workspace with one member, `acre`, whose `guard.sh` records the payload
/// it was handed in `<root>/fired`, and the state an older homma left.
fn workspace() -> Ws {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ws");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let acre = root.join("acre");
    git_init(&acre);
    homma_fixture::script(
        &acre.join(".claude/hooks/guard.sh"),
        &format!("#!/bin/sh\ncat > '{}'\n", root.join("fired").display()),
        0o755,
    );
    std::fs::write(
        acre.join(".claude/settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/guard.sh"}]}]}}"#,
    )
    .unwrap();
    homma_fixture::script(
        &root.join(".claude/hooks/acre--guard.sh"),
        &format!("#!/bin/sh\n{MARK}\n"),
        0o755,
    );
    std::fs::write(root.join(".claude/settings.local.json"), OLD_SETTINGS).unwrap();
    let cfg = dir.path().join("homma.toml");
    std::fs::write(
        &cfg,
        format!(
            "[workspace]\nname = \"ws\"\npath = \"{}\"\n\n[forges.github]\nkind = \"github\"\nbase_url = \"https://github.com\"\napi_url = \"https://api.github.com\"\n",
            root.display()
        ),
    )
    .unwrap();
    Ws {
        _dir: dir,
        root,
        cfg,
        home,
    }
}

fn regen(ws: &Ws, path: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_homma-engine"))
        .env("HOME", &ws.home)
        .env("PATH", path)
        .args(["--config", ws.cfg.to_str().unwrap(), "agent", "regen"])
        .args(["--skip-cargo-mock", "--skip-configs", "--skip-surfaces"])
        .output()
        .unwrap()
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A `PATH` holding git and a shell, and no claude-shook.
fn bare_path() -> String {
    "/usr/bin:/bin".to_string()
}

fn manifest(ws: &Ws) -> PathBuf {
    ws.root.join(".claude/shook/manifests/homma.toml")
}

fn left_as_it_was(ws: &Ws) {
    assert!(
        ws.root.join(".claude/hooks/acre--guard.sh").is_file(),
        "the old wrapper went"
    );
    assert_eq!(
        std::fs::read_to_string(ws.root.join(".claude/settings.local.json")).unwrap(),
        OLD_SETTINGS
    );
    assert!(!manifest(ws).exists(), "a manifest was left behind");
}

#[test]
fn without_claude_shook_the_regen_refuses_the_hooks_and_leaves_them_as_they_were() {
    let ws = workspace();
    let out = regen(&ws, &bare_path());
    assert!(!out.status.success(), "{}", said(&out));
    assert!(
        said(&out).contains("`claude-shook` is not on PATH"),
        "{}",
        said(&out)
    );
    left_as_it_was(&ws);
    // asked before anything is written: not even the gate's script
    assert!(
        !ws.root
            .join(".claude/hooks/_workspace--mockspace-gate.sh")
            .exists()
    );
}

#[test]
fn a_front_that_refuses_leaves_the_hooks_as_they_were() {
    let ws = workspace();
    let bin = ws.root.parent().unwrap().join("bin");
    homma_fixture::script(
        &bin.join("claude-shook"),
        "#!/bin/sh\necho nope, not this >&2\nexit 1\n",
        0o755,
    );
    let out = regen(&ws, &format!("{}:{}", bin.display(), bare_path()));
    assert!(!out.status.success(), "{}", said(&out));
    assert!(said(&out).contains("nope, not this"), "{}", said(&out));
    left_as_it_was(&ws);
}

#[test]
fn a_front_that_takes_them_gets_every_row_and_the_old_shape_is_swept() {
    let ws = workspace();
    let bin = ws.root.parent().unwrap().join("bin");
    let out = regen(&ws, &homma_fixture::front_on_path(&bin));
    assert!(out.status.success(), "{}", said(&out));
    let rows: toml::Table = std::fs::read_to_string(manifest(&ws))
        .unwrap()
        .parse()
        .unwrap();
    let pre = rows["PreToolUse"].as_array().unwrap();
    let names: Vec<&str> = pre.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["_workspace--mockspace-gate", "acre--guard.sh"],
        "the gate first"
    );
    assert_eq!(
        pre[1]["run"].as_array().unwrap()[0].as_str(),
        Some("{root}/acre/.claude/hooks/guard.sh")
    );
    assert_eq!(
        pre[1]["paths"].as_array().unwrap()[0].as_str(),
        Some("acre")
    );
    assert_eq!(pre[1]["matcher"].as_str(), Some("Edit"));
    let start = rows["SessionStart"].as_array().unwrap();
    assert_eq!(start[0]["name"].as_str(), Some("_git-hooks"));
    assert_eq!(start[0]["dir"].as_str(), Some("."));
    // the wrapper and its registration went; what somebody else wrote stays
    assert!(!ws.root.join(".claude/hooks/acre--guard.sh").exists());
    assert!(
        ws.root
            .join(".claude/hooks/_workspace--mockspace-gate.sh")
            .is_file(),
        "the gate's script is its row's program"
    );
    let local = std::fs::read_to_string(ws.root.join(".claude/settings.local.json")).unwrap();
    assert!(!local.contains("acre--guard.sh"), "{local}");
    assert!(
        local.contains("mine.sh") && local.contains("Artifact"),
        "{local}"
    );
}

/// The move end to end with the real front. Needs `CLAUDE_SHOOK` naming the
/// directory claude-shook is in and `SHOOK_BASE` naming the base it runs on.
#[test]
fn with_the_real_front_one_call_runs_the_members_hook_only_where_the_call_lands_in_it() {
    let Some(front) = std::env::var_os("CLAUDE_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: CLAUDE_SHOOK names no directory holding claude-shook");
        return;
    };
    let ws = workspace();
    let path = format!("{}:{}", front.display(), bare_path());
    let out = regen(&ws, &path);
    assert!(out.status.success(), "{}", said(&out));
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(ws.root.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
    let ours: Vec<&serde_json::Value> = pre
        .iter()
        .filter(|g| {
            g["hooks"][0]["command"]
                .as_str()
                .is_some_and(|c| c.contains("exec claude-shook run"))
        })
        .collect();
    assert_eq!(ours.len(), 1, "one registration for the event: {settings}");
    let cmd = ours[0]["hooks"][0]["command"].as_str().unwrap();
    let call = |file: &str| {
        let _ = std::fs::remove_file(ws.root.join("fired"));
        let payload = format!(
            r#"{{"tool_name":"Edit","cwd":"{}","tool_input":{{"file_path":"{file}"}}}}"#,
            ws.root.display()
        );
        let mut child = Command::new("/bin/sh")
            .args(["-c", cmd])
            .env("PATH", &path)
            .env("CLAUDE_PROJECT_DIR", &ws.root)
            .current_dir(&ws.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", said(&out));
        std::fs::read_to_string(ws.root.join("fired")).ok()
    };
    let landed = call("acre/src/lib.rs").expect("an edit in the member ran its hook");
    assert!(
        landed.contains("acre/src/lib.rs"),
        "the hook was handed the payload: {landed}"
    );
    assert_eq!(
        call("elsewhere/x.rs"),
        None,
        "an edit elsewhere ran the member's hook"
    );
}

#[test]
fn a_regen_of_one_member_still_carries_every_member_into_the_one_manifest() {
    let ws = workspace();
    let krill = ws.root.join("krill");
    git_init(&krill);
    homma_fixture::script(
        &krill.join(".claude/hooks/check.sh"),
        "#!/bin/sh\ncat >/dev/null\n",
        0o755,
    );
    std::fs::write(
        krill.join(".claude/settings.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":".claude/hooks/check.sh"}]}]}}"#,
    )
    .unwrap();
    let bin = ws.root.parent().unwrap().join("bin");
    let out = Command::new(env!("CARGO_BIN_EXE_homma-engine"))
        .env("HOME", &ws.home)
        .env("PATH", homma_fixture::front_on_path(&bin))
        .args(["--config", ws.cfg.to_str().unwrap(), "agent", "regen", "--repo", "acre"])
        .args(["--skip-cargo-mock", "--skip-configs", "--skip-surfaces"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", said(&out));
    let rows: toml::Table = std::fs::read_to_string(manifest(&ws))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        rows["Stop"].as_array().unwrap()[0]["name"].as_str(),
        Some("krill--check.sh")
    );
    assert!(
        rows["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"].as_str() == Some("acre--guard.sh"))
    );
}
