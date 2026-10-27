//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Tests of the workspace gate: the generated script run under bash, against
//! a stub `homma` that answers from what the real probe says about each tree.

use std::fs;
use std::path::Path;

/// A `Root` over a test workspace, denying nothing that a test uses.
///
/// The real code path, not a variant of it: these go through
/// `Root::contain` exactly as production does, which is what makes the
/// containment they assert mean anything.
fn test_root(workspace: &Path) -> homma_api::Root {
    homma_api::Root::new(
        &homma_api::AbsPath::new(workspace).expect("a tempdir path is absolute"),
        homma_api::Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap()),
    )
    .expect("a tempdir is a legitimate root")
}
use super::*;

#[test]
fn install_writes_executable_script_and_returns_entry() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let repos = vec![
        ("arvo".to_string(), "arvo".to_string()),
        ("notko".to_string(), "notko".to_string()),
    ];
    let entry = install_workspace_gate(&test_root(workspace), &repos).unwrap();
    assert_eq!(entry.matcher, "Bash");
    assert_eq!(
        entry.command, "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/_workspace--mockspace-gate.sh",
        "the registered command must name the host's project-root placeholder",
    );
    assert!(
        !entry.command.contains(workspace.to_str().unwrap()),
        "the command named the workspace that generated it: {}",
        entry.command,
    );

    let script = workspace.join(".claude/hooks/_workspace--mockspace-gate.sh");
    assert!(script.exists());
    let body = fs::read_to_string(&script).unwrap();
    assert!(body.starts_with("#!/usr/bin/env bash"));
    assert!(body.contains("'arvo|arvo'"));
    assert!(body.contains("'notko|notko'"));
    assert!(
        !body.contains(workspace.to_str().unwrap()),
        "the gate baked the generating workspace's path"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "script not executable: mode {mode:o}");
    }
}

#[test]
fn is_workspace_gate_command_matches_managed_path() {
    assert!(is_workspace_gate_command(
        ".claude/hooks/_workspace--mockspace-gate.sh"
    ));
}

#[test]
fn is_workspace_gate_command_rejects_other_paths() {
    assert!(!is_workspace_gate_command(
        ".claude/hooks/arvo--no-alloc.sh"
    ));
}

#[test]
fn a_manifest_path_carrying_a_curdir_component_resolves_to_the_repo() {
    // The gate's repo table is the one place a manifest path reaches an
    // emitted script without passing through a `strip_prefix` that would
    // have normalised it: `agent.rs` reads `local_path` straight off the
    // manifest. So `local_path = "./arvo"` is the reachable shape, and
    // `/ws/./arvo` is not a textual prefix of `/ws/arvo/src/lib.rs`, which
    // is how the longest-prefix match silently finds nothing.
    //
    // Through `install_workspace_gate` rather than through the helper, so
    // this holds the wiring. An earlier draft called `relative_str`
    // directly and passed with the wiring removed, which is what moved the
    // normalisation inside the function.
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path();
    install_workspace_gate(&test_root(ws), &[(
        "arvo".to_string(),
        "./arvo".to_string(),
    )])
    .unwrap();
    let installed =
        fs::read_to_string(ws.join(".claude/hooks/_workspace--mockspace-gate.sh")).unwrap();
    assert!(
        installed.contains("'arvo|arvo'"),
        "the gate kept a no-op path component from the manifest: {installed}"
    );
    assert!(!installed.contains("/./"));
}

#[test]
fn script_body_includes_workspace_repo_paths() {
    let repos = vec![
        ("arvo".to_string(), "/abs/arvo".to_string()),
        ("hilavitkutin".to_string(), "/abs/hilavitkutin".to_string()),
    ];
    let body = gate_script(&repos);
    assert!(body.contains("'arvo|/abs/arvo'"));
    assert!(body.contains("'hilavitkutin|/abs/hilavitkutin'"));
}

#[test]
fn script_body_handles_empty_repo_list() {
    let body = gate_script(&[]);
    assert!(body.contains("REPOS=("));
}

// -------------------------------------------------------------------
// End-to-end bash-execution tests for the rendered gate script. The
// script reads JSON from stdin via `jq`, walks the workspace's
// member-repo table to find a covering repo, then asks homma about that
// repo's mockspace adoption surfaces (the declared mock dir and the
// launcher pin) and reads `core.hooksPath` itself. All exit codes are 0;
// the allow/deny signal travels through stdout's JSON payload.
//
// Each test renders the script via gate_script(), writes it into a
// workspace beside a stub `homma`, builds a synthetic repo tree with
// whichever surfaces the case wants present, then invokes `bash <script>`
// with synthesised INPUT JSON. The stub answers `agent status` with what
// the real probe says about the tree the test built, so the gate is held
// against the one reader and not against a second copy of it.
//
// **They used to skip when bash or jq were missing, and a skip here is
// indistinguishable from a pass.** Six tests returned early and reported
// green having run nothing, on the same machine where the script they
// cover allowed every commit for the same reason. `bash` is required
// outright now, since a bash script cannot be covered without it. `jq` is
// a case rather than an excuse: `run_gate` takes the PATH to run under, so
// its absence is one more thing the suite asserts about.
// -------------------------------------------------------------------

/// Synthesise an INPUT JSON payload matching the Claude Code Bash
/// tool-call shape the gate script parses.
fn input_json(command: &str, cwd: &str) -> String {
    serde_json::json!({
        "tool_input": {
            "command": command,
            "cwd": cwd,
        }
    })
    .to_string()
}

/// Run the gate script with the given INPUT, with a stub `homma` on the
/// PATH, and return stdout.
fn run_gate(repos: &[(String, String)], input: &str) -> String {
    run_gate_on_path(repos, input, None)
}

/// The `agent status` answer homma would give for `repo`, written by the
/// real probe rather than by hand, so the gate is tested against what the
/// one reader says about the tree the test built.
fn status_json(name: &str, repo: &Path) -> String {
    let report = crate::cmd::agent::status::AgentStatusReport {
        repos: vec![crate::cmd::agent::status::probe(name, repo)],
    };
    serde_json::to_string(&report).unwrap()
}

/// Write one `status-<repo>.json` per repo of the table into `stub_dir`,
/// which is where the stub `homma` of `a_path_with_homma` reads them from.
fn write_status_fixtures(stub_dir: &Path, repos: &[(String, String)]) {
    fs::create_dir_all(stub_dir).unwrap();
    for (name, path) in repos {
        fs::write(
            stub_dir.join(format!("status-{name}.json")),
            status_json(name, Path::new(path)),
        )
        .unwrap();
    }
}

/// The same, with `path` replacing `PATH` for the child.
///
/// The parameter exists so the missing-`jq` case is a test rather than a
/// reason to skip five others. Passing a directory holding only `bash` and
/// its dependencies reproduces a machine without `jq` on one that has it.
///
/// The script sits where the generated one does, `<ws>/.claude/hooks/`, in a
/// workspace with a manifest, since it asks homma about the repo and only
/// does that where there is a workspace to ask in.
fn run_gate_on_path(repos: &[(String, String)], input: &str, path: Option<&str>) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Required rather than skipped. This suite covers a bash script, so
    // without bash it covers nothing, and saying so is the only honest
    // report available.
    assert!(
        Command::new("bash").arg("--version").output().is_ok(),
        "these tests cover a bash script and bash is not on PATH"
    );

    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("workspace");
    let hooks = ws.join(".claude").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(ws.join("homma.toml"), "content_repo = \"x\"\n").unwrap();
    let script_path = hooks.join("gate.sh");
    fs::write(&script_path, gate_script(repos)).unwrap();

    let stub_dir = dir.path().join("stub");
    write_status_fixtures(&stub_dir, repos);
    let own_path;
    let path = match path {
        Some(p) => p,
        None => {
            own_path = a_path_with_homma(dir.path(), Some("exit 0"));
            &own_path
        },
    };

    let mut child = Command::new("bash")
        .arg(&script_path)
        .env("PATH", path)
        .env("HOMMA_STUB_DIR", &stub_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bash would not start");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("bash would not finish");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Build a fake repo tree with a tunable set of mockspace surfaces.
/// Returns the repo's absolute path.
fn fake_repo(
    parent: &Path,
    name: &str,
    with_mock_dir: bool,
    with_hooks_path: bool,
) -> std::path::PathBuf {
    let repo = parent.join(name);
    fs::create_dir_all(&repo).unwrap();

    if with_mock_dir {
        fs::create_dir_all(repo.join("mock")).unwrap();
    }

    if with_hooks_path {
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "core.hooksPath", "mock/target/hooks"])
            .current_dir(&repo)
            .output()
            .unwrap();
    }

    repo
}

/// A PATH holding what these scripts need and no `jq`.
///
/// Built by symlinking each program rather than by trimming the real PATH,
/// because there is no way to un-find something on a PATH and `jq` sits in a
/// different directory on every machine.
///
/// The list is what the generated scripts actually shell out to. It is short
/// because most of what they use is a bash builtin, and it is explicit
/// because a PATH missing one of these fails somewhere unrelated: the first
/// version of this held `bash` alone, and `dirname` came back empty, so the
/// script resolved its workspace to `/` and reached the check under test by
/// accident.
pub(crate) fn a_path_without_jq(dir: &Path) -> String {
    use std::process::Command;
    let bin = dir.join("bin-without-jq");
    fs::create_dir_all(&bin).unwrap();
    for program in ["bash", "dirname", "cat"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {program}")])
            .output()
            .expect("no shell");
        let found = String::from_utf8_lossy(&found.stdout).trim().to_string();
        assert!(!found.is_empty(), "{program} is not on PATH");
        std::os::unix::fs::symlink(&found, bin.join(program)).unwrap();
    }
    let path = bin.to_string_lossy().into_owned();
    // That it really has no jq, rather than being assumed to. A PATH that
    // happened to reach one would leave every caller testing the ordinary
    // path twice and reporting green.
    assert!(
        !Command::new(bin.join("bash"))
            .args(["-c", "command -v jq"])
            .env("PATH", &path)
            .status()
            .unwrap()
            .success(),
        "the restricted PATH still reaches jq"
    );
    path
}

/// **The gate denies rather than allowing silently when `jq` is missing.**
///
/// It exited 0 with no output instead: both reads of the input carried
/// `2>/dev/null`, so the command line came back empty and the script took
/// its own out-of-scope branch, which is the same exit 0 it produces when
/// it has looked and found nothing wrong. Nothing outside could tell the
/// two apart, and every commit and every push in such a workspace went
/// through unchecked.
#[test]
fn gate_e2e_without_jq_denies_rather_than_allowing_silently() {
    let dir = tempfile::tempdir().unwrap();
    let path = a_path_without_jq(dir.path());
    // Full adoption: the case that is *supposed* to be allowed, so a deny
    // here can only be the missing tool and not the drift check.
    let repo = fake_repo(dir.path(), "arvo", true, true);
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"mock\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());

    // The control, on the inherited PATH: this exact call is allowed when
    // `jq` is there. Without it the assertion below cannot tell a
    // missing-tool deny from a gate that denies everything.
    let with_jq = run_gate(&repos, &input);
    assert!(
        !with_jq.contains("\"permissionDecision\":\"deny\""),
        "control: full adoption is allowed when jq is present; got: {with_jq}"
    );

    let out = run_gate_on_path(&repos, &input, Some(&path));
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "a workspace without jq allowed a commit the gate could not read; got: {out}"
    );
    assert!(
        out.contains("jq"),
        "the refusal has to name the missing tool, or nobody can fix it: {out}"
    );
}

#[test]
fn gate_e2e_launcher_pin_counts_as_config_marker() {
    // launcher+pin model: no `[alias] mock`, but a root mockspace.toml with
    // a `mockspace_branch` pin. With mock/ + hooksPath present, that is full
    // adoption and must not deny.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, true);
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"mock\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "launcher pin + mock/ + hooks is full adoption; should not deny; got: {out}",
    );
}

#[test]
fn gate_e2e_declared_mock_dir_counts_as_the_mock_dir() {
    // sodapop declares `mock_dir = "linux"`. A gate looking for a literal
    // `mock/` reads it as missing the directory and denies a fully adopted
    // repo.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "sodapop", false, true);
    fs::create_dir_all(repo.join("linux")).unwrap();
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"linux\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("sodapop".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "a declared mock dir with pin and hooks is full adoption; got: {out}",
    );
}

#[test]
fn gate_e2e_declared_mock_dir_that_is_absent_is_named_in_the_deny() {
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "sodapop", false, true);
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"linux\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("sodapop".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "got: {out}"
    );
    assert!(out.contains("/linux/ is missing"), "got: {out}");
}

#[test]
fn gate_e2e_a_mock_dir_with_no_toml_is_left_alone() {
    // mockspace's own tree shape: `mock/` and nothing else. Without a
    // `mockspace.toml` it has not adopted mockspace, and a deny naming
    // `homma agent regen` sends somebody to a command that skips it.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, false);
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "no toml means no adoption; got: {out}",
    );
}

#[test]
fn gate_e2e_a_toml_under_the_mock_dir_counts_as_adopting() {
    // ikiuni keeps its config at `mock/mockspace.toml`. Drift fixture with
    // the pin under `mock/` and no hooks: the gate must see the toml and
    // deny, not treat the repo as unadopted.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "ikiuni", true, false);
    fs::write(
        repo.join("mock/mockspace.toml"),
        "mockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("ikiuni".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "a toml under mock/ is adoption, and the hooks are missing; got: {out}",
    );
    assert!(out.contains("core.hooksPath"), "got: {out}");
}

#[test]
fn gate_e2e_the_root_toml_wins_over_the_one_under_the_mock_dir() {
    // Both exist and disagree about `mock_dir`; the root one is the one
    // read, as the status probe reads it.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "sodapop", true, true);
    fs::create_dir_all(repo.join("linux")).unwrap();
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"linux\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    fs::write(
        repo.join("mock/mockspace.toml"),
        "mock_dir = \"elsewhere\"\n",
    )
    .unwrap();
    let repos = vec![("sodapop".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "the root toml declares linux, which exists; got: {out}",
    );
}

#[test]
fn gate_e2e_partial_adoption_emits_deny() {
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "partial adoption (mock/ only) should deny; got: {out}",
    );
    assert!(
        out.contains("no launcher pin"),
        "deny reason should name the missing config marker; got: {out}",
    );
    assert!(
        out.contains("Add a launcher pin") && out.contains("homma agent regen"),
        "the fix should name the pin first and the rest after it; got: {out}",
    );
}

#[test]
fn gate_e2e_zero_adoption_emits_no_deny() {
    // A toml and nothing else: no mock dir, no pin, no alias, no
    // hooksPath. Adopted in name only, with no surface present, which is
    // not drift, so the gate stays out of the way.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", false, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "zero adoption should be a silent allow; got: {out}",
    );
}

/// Point a repo's `core.hooksPath` at the directory git-shook generates,
/// which is what `git shook install` writes and where its dispatcher runs
/// the entries a repository's manifests declare.
fn point_hooks_path_at_shook(repo: &Path) {
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(repo)
        .output()
        .unwrap();
    let common = std::process::Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(repo)
        .output()
        .unwrap();
    let common = String::from_utf8_lossy(&common.stdout).trim().to_string();
    std::process::Command::new("git")
        .args(["config", "core.hooksPath", &format!("{common}/shook/hooks")])
        .current_dir(repo)
        .output()
        .unwrap();
}

#[test]
fn gate_e2e_a_git_shook_hooks_path_is_not_a_mockspace_surface() {
    // A repository that never adopted mockspace and did adopt git-shook:
    // no mock/, no alias, and a hooks path pointing at git-shook's own
    // dispatcher directory. Counting that as mockspace's reads one surface
    // of three, which is drift, and refuses every commit in a repository
    // with nothing wrong with it and nothing `homma agent regen` can fix.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", false, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    point_hooks_path_at_shook(&repo);
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "git-shook's hooks path is not mockspace's; got: {out}",
    );
}

#[test]
fn gate_e2e_a_hooks_path_that_is_not_git_shooks_still_counts() {
    // The boundary, and the reason the check compares against the
    // directory git-shook generates rather than matching a spelling: any
    // other hooks path is still evidence that mockspace's hooks are wired,
    // so a repository carrying that and nothing else is still drift.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", false, true);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "a hooks path that is not git-shook's is still a surface; got: {out}",
    );
}

#[test]
fn gate_e2e_an_alias_does_not_stand_in_for_the_launcher_pin() {
    // The legacy `cargo mock` alias is not a surface: the launcher answers
    // that command, and `cargo mock` refuses to run without a pin. With the
    // mock dir and the hooks path present and the alias too, a toml with no
    // pin is drift, and the fix is the pin, not `homma agent regen`, which
    // runs `cargo mock`.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, true);
    fs::create_dir_all(repo.join(".cargo")).unwrap();
    fs::write(
        repo.join(".cargo/config.toml"),
        "[alias]\nmock = \"run -p mock-cli --\"\n",
    )
    .unwrap();
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        out.contains("\"permissionDecision\":\"deny\""),
        "an alias is not the pin; got: {out}",
    );
    assert!(out.contains("no launcher pin"), "{out}");
    assert!(out.contains("Add a launcher pin"), "{out}");
    assert!(
        !out.contains("homma agent regen"),
        "regen cannot run cargo mock without a pin and must not be the advice; got: {out}",
    );
}

#[test]
fn gate_e2e_non_git_command_is_silent_allow() {
    // Out of scope: command is not git commit / git push. The repo is drift (a
    // toml and a mock directory, no pin, no hooks path), which is what the
    // control below shows the gate refusing, so the allow can only be the filter.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("ls -la", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "non-git command should be silent allow; got: {out}",
    );
    let control = run_gate(
        &repos,
        &input_json("git commit -m hi", &repo.to_string_lossy()),
    );
    assert!(
        control.contains("\"permissionDecision\":\"deny\""),
        "the same repo must be refused on a commit; got: {control}",
    );
}

// -------------------------------------------------------------------
// The shared-config arm.
//
// Every case below puts the repo at zero mockspace adoption, so the
// bootstrap arm contributes nothing and whatever the gate says comes from
// the config check alone. Without that isolation a pass could be the
// bootstrap arm firing and the config arm never running at all.
// -------------------------------------------------------------------

/// A PATH holding what the script shells out to, plus a `homma` behaving
/// as `homma_body` says.
///
/// `None` leaves `homma` off the PATH entirely, which is the case where
/// the gate cannot ask at all.
fn a_path_with_homma(dir: &Path, homma_body: Option<&str>) -> String {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let bin = dir.join("bin-with-homma");
    fs::create_dir_all(&bin).unwrap();
    // Everything the generated script shells out to, not only the pieces
    // this case is about. A PATH missing one of them fails somewhere
    // unrelated and quietly: without `sed` the deny payload is built and
    // then emitted with an empty reason, which still looks like a refusal
    // and tells the reader nothing.
    for program in ["bash", "dirname", "cat", "jq", "sed", "grep", "awk", "git"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {program}")])
            .output()
            .expect("no shell");
        let found = String::from_utf8_lossy(&found.stdout).trim().to_string();
        assert!(!found.is_empty(), "{program} is not on PATH");
        std::os::unix::fs::symlink(&found, bin.join(program)).unwrap();
    }
    if let Some(body) = homma_body {
        let at = bin.join("homma");
        // `agent status --repo <name>` answers from the fixture the runner
        // wrote with the real probe, the one shared stub for every case; a
        // case's own `body` answers whatever else is asked of homma.
        let preamble = r#"case " $* " in
  *" agent status "*)
repo=""; prev=""
for a in "$@"; do
  [ "$prev" = "--repo" ] && repo="$a"
  prev="$a"
done
[ -f "$HOMMA_STUB_DIR/stderr-noise" ] && echo "using the last known revision for dev" >&2
cat "$HOMMA_STUB_DIR/status-$repo.json" && exit 0
exit 3 ;;
esac"#;
        fs::write(&at, format!("#!/usr/bin/env bash\n{preamble}\n{body}\n")).unwrap();
        fs::set_permissions(&at, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = bin.to_string_lossy().into_owned();
    // That the stub is reachable, or absent, as the case intends. Assuming
    // either is how a test ends up covering the other branch in silence.
    let reachable = Command::new(bin.join("bash"))
        .args(["-c", "command -v homma"])
        .env("PATH", &path)
        .status()
        .unwrap()
        .success();
    assert_eq!(
        reachable,
        homma_body.is_some(),
        "the restricted PATH does not have the homma this case wants"
    );
    path
}

/// Run the gate from inside a real-shaped workspace.
///
/// The script goes where the generated one goes, `<ws>/.claude/hooks/`,
/// because it finds its workspace from its own location and a script
/// anywhere else resolves to the wrong one. A `homma.toml` is written
/// unless `with_manifest` is false, since that file is what tells the
/// script there is a workspace to ask about at all.
fn run_gate_in_a_workspace(homma_body: Option<&str>, with_manifest: bool) -> String {
    run_gate_in_a_workspace_with(homma_body, with_manifest, Answer::Probe, false)
}

/// What the stub `homma` says to `agent status`.
enum Answer {
    /// What the real probe says about the tree the test built.
    Probe,
    /// Nothing: homma failing to report on the repo.
    Absent,
    /// This text, exit 0: homma answering with something that is not the report.
    Literal(&'static str),
}

/// The same, with `status` saying what the stub answers to `agent status`.
/// `stderr_noise` makes the stub write a line to stderr before it answers, as
/// the launcher does when it is offline past its cache or builds a moved pin.
fn run_gate_in_a_workspace_with(
    homma_body: Option<&str>,
    with_manifest: bool,
    status: Answer,
    stderr_noise: bool,
) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("workspace");
    let hooks = ws.join(".claude").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    if with_manifest {
        fs::write(ws.join("homma.toml"), "content_repo = \"x\"\n").unwrap();
    }
    // Zero adoption on purpose: no mock/, no alias, no hooksPath.
    let repo = fake_repo(&ws, "arvo", false, false);

    let script = hooks.join("gate.sh");
    fs::write(
        &script,
        gate_script(&[("arvo".to_string(), "arvo".to_string())]),
    )
    .unwrap();

    let input = input_json("git commit -m x", &repo.to_string_lossy());
    let path = a_path_with_homma(dir.path(), homma_body);
    let stub_dir = dir.path().join("stub");
    fs::create_dir_all(&stub_dir).unwrap();
    if stderr_noise {
        fs::write(stub_dir.join("stderr-noise"), "").unwrap();
    }
    match status {
        Answer::Probe => {
            write_status_fixtures(&stub_dir, &[(
                "arvo".to_string(),
                repo.to_string_lossy().to_string(),
            )])
        },
        Answer::Literal(text) => fs::write(stub_dir.join("status-arvo.json"), text).unwrap(),
        Answer::Absent => {},
    }
    let mut child = Command::new("bash")
        .arg(&script)
        .env("PATH", path)
        .env("HOMMA_STUB_DIR", &stub_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bash would not start");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("bash would not finish");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn gate_allows_when_the_shared_configs_are_in_order() {
    // `exit 0` is what `repo config check` does when nothing blocks.
    let out = run_gate_in_a_workspace(Some("exit 0"), true);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "a repo owing nothing was refused: {out}"
    );
}

#[test]
fn gate_denies_when_a_required_config_is_missing_and_names_the_fix() {
    // Non-zero is what it does when something blocks, and what it printed
    // is what the person needs to see.
    let out = run_gate_in_a_workspace(
        Some("echo 'arvo'; echo '  deny.toml is missing, and is required here'; exit 1"),
        true,
    );
    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(out.contains("deny.toml is missing"), "{out}");
    assert!(
        out.contains("homma repo config init --repo arvo"),
        "the refusal did not name the command that fixes it: {out}"
    );
}

#[test]
fn gate_denies_when_the_check_could_not_run_and_does_not_send_you_to_init() {
    // Exit 2 is homma saying it could not run, and the commonest reason on
    // this path is this workspace's own manifest failing to parse, which is
    // nothing the repo being committed to can fix. It still denies, for the
    // reason the arms around it deny. What it must not do is name `init`,
    // which reads the same manifest through the same function and fails
    // identically.
    let out = run_gate_in_a_workspace(
        Some("echo 'error: loading config from /w/homma.toml'; exit 2"),
        true,
    );
    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(
        out.contains("could not be checked"),
        "the refusal did not say the check never ran: {out}"
    );
    assert!(
        out.contains("loading config from"),
        "and did not carry what homma printed, which is the only clue: {out}"
    );
    assert!(
        out.contains("homma.toml"),
        "and did not name the manifest to read: {out}"
    );
    assert!(
        !out.contains("repo config init"),
        "it sent somebody to a command that fails on the same manifest: {out}"
    );
}

#[test]
fn gate_denies_when_it_cannot_ask_at_all() {
    // The same reasoning as the missing-`jq` case: a check that could not
    // run must never be indistinguishable from one that ran and found
    // nothing. Allowing here would mean a workspace with no homma installed
    // silently stops enforcing anything.
    let out = run_gate_in_a_workspace(None, true);
    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(out.contains("homma is not on PATH"), "{out}");
}

#[test]
fn gate_denies_when_homma_cannot_report_on_the_repo() {
    // homma is there and the shared configs are in order, but `agent
    // status` gives no answer. The gate holds no reader of the toml of its
    // own to fall back on, so it refuses and says what it could not read,
    // rather than allowing as though it had looked and found nothing.
    let out = run_gate_in_a_workspace_with(Some("exit 0"), true, Answer::Absent, false);
    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(
        out.contains("the mockspace surfaces could not be read"),
        "{out}"
    );
    // The control: the same workspace with the answer present is allowed.
    let ok = run_gate_in_a_workspace_with(Some("exit 0"), true, Answer::Probe, false);
    assert!(!ok.contains("\"permissionDecision\":\"deny\""), "{ok}");
}

#[test]
fn gate_skips_the_config_check_where_there_is_no_workspace_to_ask_about() {
    // No manifest means no workspace, so there is nothing to resolve the
    // repo against. Refusing would be the gate reporting the absence of its
    // own input as a fault in somebody's repo.
    let out = run_gate_in_a_workspace(None, false);
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "a repo was refused over the gate's own missing input: {out}"
    );
}

#[test]
fn the_two_arms_are_reported_together_rather_than_one_hiding_the_other() {
    // A repo can be wrong on both counts and should hear about both. The
    // bootstrap arm needs partial adoption to fire, which is what this one
    // builds instead of the zero-adoption tree the others use.
    use std::io::Write;
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("workspace");
    let hooks = ws.join(".claude").join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(ws.join("homma.toml"), "content_repo = \"x\"\n").unwrap();
    // Partial adoption: a toml and mock/ present, no pin, no hooksPath.
    let repo = fake_repo(&ws, "arvo", true, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();

    let script = hooks.join("gate.sh");
    fs::write(
        &script,
        gate_script(&[("arvo".to_string(), "arvo".to_string())]),
    )
    .unwrap();
    let path = a_path_with_homma(
        dir.path(),
        Some("echo '  deny.toml is missing, and is required here'; exit 1"),
    );
    let input = input_json("git commit -m x", &repo.to_string_lossy());
    let stub_dir = dir.path().join("stub");
    write_status_fixtures(&stub_dir, &[(
        "arvo".to_string(),
        repo.to_string_lossy().to_string(),
    )]);
    let mut child = Command::new("bash")
        .arg(&script)
        .env("PATH", path)
        .env("HOMMA_STUB_DIR", &stub_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bash would not start");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("bash would not finish");
    let out = String::from_utf8_lossy(&out.stdout).into_owned();

    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(
        out.contains("core.hooksPath is not set"),
        "the bootstrap arm is missing: {out}"
    );
    assert!(
        out.contains("deny.toml is missing"),
        "the config arm is missing: {out}"
    );
    // And each arm's advice, not only its problems: the second must not
    // overwrite the first.
    assert!(
        out.contains("Add a launcher pin"),
        "the pin advice was overwritten: {out}"
    );
    assert!(
        out.contains("homma repo config init --repo arvo"),
        "the config advice is missing: {out}"
    );
}

#[test]
fn gate_reads_the_answer_from_stdout_and_ignores_what_homma_says_on_stderr() {
    // In production `homma` is the launcher, which writes a line to stderr on a
    // successful run when it is offline past its cache, builds the engine for a
    // moved pin, or updates itself. None of that is the answer, and folding it
    // into the JSON made the gate refuse every commit in every member repo.
    let noisy = run_gate_in_a_workspace_with(Some("exit 0"), true, Answer::Probe, true);
    assert!(
        !noisy.contains("\"permissionDecision\":\"deny\""),
        "stderr from homma was read as part of its answer: {noisy}"
    );
}

#[test]
fn gate_e2e_a_pin_alone_is_a_surface_and_the_rest_missing_is_drift() {
    // The pin counts toward adoption: a toml with a pin and no mock directory
    // and no hooks path is one surface of three, which is neither none nor all.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", false, false);
    fs::write(repo.join("mockspace.toml"), "mockspace_branch = \"dev\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(out.contains("\"permissionDecision\":\"deny\""), "{out}");
    assert!(
        out.contains("is missing but other mockspace surfaces"),
        "{out}"
    );
    assert!(
        !out.contains("no launcher pin"),
        "the pin is there and must not be named as missing: {out}"
    );
}

#[test]
fn gate_denies_when_homma_answers_with_something_that_is_not_the_report() {
    // Exit 0 and output that is not the status document, or one without the
    // fields the gate reads: the gate cannot tell what the repo has, so it
    // refuses rather than reading the missing facts as absent surfaces.
    for text in ["not json at all", "{\"repos\": []}", "{\"repos\": [{\"surfaces\": {}}]}"] {
        let out = run_gate_in_a_workspace_with(Some("exit 0"), true, Answer::Literal(text), false);
        assert!(
            out.contains("\"permissionDecision\":\"deny\""),
            "{text}: {out}"
        );
        assert!(
            out.contains("the mockspace surfaces could not be read"),
            "{text}: {out}"
        );
    }
}

#[test]
fn gate_reads_the_config_findings_from_stdout_and_not_what_homma_says_on_stderr() {
    // The same class as the status read: a launcher line on stderr is not a
    // finding about the repo, and was listed as a problem beside the real one.
    let out = run_gate_in_a_workspace(
        Some(
            "echo 'homma: offline past the cache, using the last known revision' >&2; echo '  deny.toml is missing'; exit 1",
        ),
        true,
    );
    assert!(out.contains("deny.toml is missing"), "{out}");
    assert!(
        !out.contains("offline past the cache"),
        "stderr was listed as a problem: {out}"
    );
}

#[test]
fn gate_e2e_an_empty_mock_dir_does_not_shift_the_facts() {
    // `mock_dir = ""` names the repository root, which exists. An empty field
    // in the facts the gate splits must stay an empty field, or the ones after
    // it move and a correct answer from homma reads as unreadable.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", false, true);
    fs::write(
        repo.join("mockspace.toml"),
        "mock_dir = \"\"\nmockspace_branch = \"dev\"\n",
    )
    .unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(
        !out.contains("could not be read"),
        "an empty field made homma's answer unreadable: {out}"
    );
    assert!(
        !out.contains("\"permissionDecision\":\"deny\""),
        "the directory, the pin and the hooks path are all there: {out}"
    );
}

#[test]
fn gate_e2e_a_toml_that_does_not_parse_is_refused_for_a_missing_pin() {
    // What happens today, filed as an open question for the lead designer: the
    // status counts the toml as adopting, with the default mock directory and
    // no pin, so the gate names the pin as missing even where the line is there.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, true);
    fs::write(
        repo.join("mockspace.toml"),
        "mockspace_branch = \"dev\"\nmock_dir = \n",
    )
    .unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let input = input_json("git commit -m hi", &repo.to_string_lossy());
    let out = run_gate(&repos, &input);
    assert!(out.contains("no launcher pin"), "{out}");
}

#[test]
fn gate_e2e_a_push_on_a_drift_repo_is_refused_and_a_non_git_command_is_not() {
    // The gate covers `git push` as well as `git commit`: the same drift repo
    // is refused on both forms, including behind a `cd`, and allowed on `ls`.
    let dir = tempfile::tempdir().unwrap();
    let repo = fake_repo(dir.path(), "arvo", true, false);
    fs::write(repo.join("mockspace.toml"), "mock_dir = \"mock\"\n").unwrap();
    let repos = vec![("arvo".to_string(), repo.to_string_lossy().to_string())];
    let at = repo.to_string_lossy().to_string();
    for command in ["git push origin dev", "cd arvo && git commit -m hi"] {
        let out = run_gate(&repos, &input_json(command, &at));
        assert!(
            out.contains("\"permissionDecision\":\"deny\""),
            "{command} was not refused: {out}"
        );
    }
    let ls = run_gate(&repos, &input_json("ls -la", &at));
    assert!(!ls.contains("\"permissionDecision\":\"deny\""), "{ls}");
}
