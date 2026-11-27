use std::path::{Path, PathBuf};

use super::*;

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let out = sh::run(d.path(), "git", &["init", "--quiet", "-b", "main"]).unwrap();
    assert!(out.ok());
    for (k, v) in [("user.name", "t"), ("user.email", "t@t")] {
        assert!(sh::run(d.path(), "git", &["config", k, v]).unwrap().ok());
    }
    d
}

/// `repo`, with git-shook holding its hooks path.
fn held() -> tempfile::TempDir {
    let d = repo();
    let dir = hooks_dir(d.path()).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let set = sh::run(d.path(), "git", &[
        "config",
        "core.hooksPath",
        dir.to_str().unwrap(),
    ]);
    assert!(set.unwrap().ok());
    d
}

fn executable(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    homma_fixture::script(&p, body, 0o755);
    p
}

/// A stand-in for git-shook that records what it was called with and
/// registers, for every `[hooks.<event>]` of each local manifest, the entry
/// file the real one would write.
fn fake_shook(bin: &Path, log: &Path) {
    executable(
        bin,
        "git-shook",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\n\
             g=$(git rev-parse --path-format=absolute --git-common-dir)\n\
             for m in \"$g\"/shook/manifests/*.toml; do n=$(basename \"$m\" .toml)\n\
             for e in $(sed -n 's/^\\[hooks\\.\\(.*\\)\\]$/\\1/p' \"$m\"); do\n\
             mkdir -p \"$g/shook/entries/$e\"; : > \"$g/shook/entries/$e/$n\"\ndone; done\n",
            log = log.display()
        ),
    );
}

fn the_workspace(text: &str) -> (tempfile::TempDir, WorkspaceHooks) {
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("shook.toml"), text).unwrap();
    let found = workspace_hooks(ws.path(), Path::new("shook.toml"))
        .unwrap()
        .unwrap();
    (ws, found)
}

#[test]
fn the_gate_manifest_runs_the_release_gate_at_pre_push_and_nothing_else() {
    let parsed: toml::Table = toml::from_str(&gate_manifest()).unwrap();
    assert_eq!(parsed["name"].as_str(), Some("homma"));
    let hooks = parsed["hooks"].as_table().unwrap();
    assert_eq!(hooks.keys().collect::<Vec<_>>(), vec!["pre-push"]);
    let run: Vec<&str> = hooks["pre-push"]["run"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(run, vec!["homma", "release", "gate", "--hook"]);
}

#[test]
fn the_workspace_manifest_is_named_by_its_name_or_by_its_root() {
    let (_ws, named) = the_workspace("name = \"prose\"\n\n[hooks.pre-commit]\nrun = [\"true\"]\n");
    assert_eq!(named.name, "prose");
    assert!(named.text.contains("[hooks.pre-commit]"));

    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().join("the-workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("shook.toml"),
        "[hooks.pre-commit]\nrun = [\"true\"]\n",
    )
    .unwrap();
    let unnamed = workspace_hooks(&root, Path::new("shook.toml"))
        .unwrap()
        .unwrap();
    assert_eq!(unnamed.name, "the-workspace");
}

#[test]
fn a_workspace_with_no_manifest_has_none_and_a_broken_one_is_refused() {
    let ws = tempfile::tempdir().unwrap();
    assert_eq!(
        workspace_hooks(ws.path(), Path::new("shook.toml")).unwrap(),
        None
    );
    std::fs::write(ws.path().join("shook.toml"), "name = [\n").unwrap();
    assert!(matches!(
        workspace_hooks(ws.path(), Path::new("shook.toml")),
        Err(HookError::Manifest { .. })
    ));
}

#[test]
fn a_workspace_manifest_named_for_the_gate_or_a_path_is_refused() {
    for name in ["homma", "../escaped", ".."] {
        let ws = tempfile::tempdir().unwrap();
        std::fs::write(ws.path().join("shook.toml"), format!("name = \"{name}\"\n")).unwrap();
        let got = workspace_hooks(ws.path(), Path::new("shook.toml"));
        assert!(
            matches!(got, Err(HookError::Name { name: ref n, .. }) if n == name),
            "{name}: {got:?}"
        );
    }
}

#[test]
fn an_install_places_both_manifests_and_has_git_shook_register_those_two() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("log");
    fake_shook(bin.path(), &log);
    let (_ws, w) = the_workspace("name = \"prose\"\n\n[hooks.pre-commit]\nrun = [\"true\"]\n");

    let i = install_with(d.path(), Some(&w), Some(bin.path())).unwrap();

    let manifests = manifests_dir(d.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(manifests.join("homma.toml")).unwrap(),
        gate_manifest()
    );
    assert_eq!(
        std::fs::read_to_string(manifests.join("prose.toml")).unwrap(),
        w.text,
        "the copy is the workspace's file byte for byte"
    );
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        "install --yes --only homma --only prose"
    );
    assert_eq!(i.registered, vec!["homma", "prose"]);
}

#[test]
fn a_workspace_with_no_manifest_registers_the_gate_alone() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("log");
    fake_shook(bin.path(), &log);

    let i = install_with(d.path(), None, Some(bin.path())).unwrap();

    assert_eq!(i.registered, vec!["homma"]);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        "install --yes --only homma"
    );
}

#[test]
fn an_install_where_git_shook_does_not_hold_the_path_refuses_and_changes_nothing() {
    let d = repo();
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("log");
    fake_shook(bin.path(), &log);

    let got = install_with(d.path(), None, Some(bin.path()));

    assert!(matches!(got, Err(HookError::NotShook(_))), "{got:?}");
    assert!(
        got.unwrap_err()
            .to_string()
            .contains("run `git shook install` there")
    );
    assert!(!log.exists(), "git-shook was not called");
    assert!(!manifests_dir(d.path()).unwrap().exists());

    // a hooks path somebody else holds is not git-shook's either
    let other = d.path().join("elsewhere");
    std::fs::create_dir(&other).unwrap();
    let set = sh::run(d.path(), "git", &[
        "config",
        "core.hooksPath",
        other.to_str().unwrap(),
    ]);
    assert!(set.unwrap().ok());
    assert!(!holds_path(d.path()).unwrap());
}

#[test]
fn a_refusal_gives_each_manifest_back_as_it_was() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    executable(
        bin.path(),
        "git-shook",
        "#!/bin/sh\necho refused >&2\nexit 1\n",
    );
    let manifests = manifests_dir(d.path()).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    std::fs::write(manifests.join("homma.toml"), "the text before\n").unwrap();
    let (_ws, w) = the_workspace("name = \"prose\"\n");

    let got = install_with(d.path(), Some(&w), Some(bin.path()));

    assert!(got.unwrap_err().to_string().contains("refused"));
    assert_eq!(
        std::fs::read_to_string(manifests.join("homma.toml")).unwrap(),
        "the text before\n"
    );
    assert!(!manifests.join("prose.toml").exists());
}

#[test]
fn git_shook_registering_no_gate_is_an_error() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    executable(bin.path(), "git-shook", "#!/bin/sh\nexit 0\n");

    let got = install_with(d.path(), None, Some(bin.path()));

    assert!(
        got.unwrap_err()
            .to_string()
            .contains("registered no release gate")
    );
}

#[test]
fn git_without_the_subcommand_says_git_shook_is_not_installed() {
    let d = held();
    let empty = tempfile::tempdir().unwrap();
    // a PATH with git and nothing called git-shook
    let git = sh::run(d.path(), "sh", &["-c", "command -v git"]).unwrap();
    let git_dir = Path::new(git.stdout.trim()).parent().unwrap().to_path_buf();
    let path = format!("{}:{}", empty.path().display(), git_dir.display());
    if sh::run_with_env(d.path(), "sh", &["-c", "command -v git-shook"], &[(
        "PATH", &path,
    )])
    .unwrap()
    .ok()
    {
        eprintln!("skipped: git-shook sits beside git, so it cannot be left off PATH here");
        return;
    }
    let out = sh::run_with_env(d.path(), "git", &["shook", "install"], &[("PATH", &path)]).unwrap();
    assert!(out.stderr.contains("not a git command"));
}

#[test]
fn an_install_takes_away_what_an_older_homma_left() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    fake_shook(bin.path(), &bin.path().join("log"));
    let own = common_dir(d.path()).unwrap().join("hooks");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("pre-push"), legacy::entrypoint("pre-push")).unwrap();
    std::fs::write(own.join("pre-commit"), "#!/bin/sh\necho somebody's\n").unwrap();

    let i = install_with(d.path(), None, Some(bin.path())).unwrap();

    assert_eq!(i.removed, vec![own.join("pre-push")]);
    assert!(own.join("pre-commit").is_file(), "somebody's hook stays");
}

const INSTALL_IN_CHILD: &str = "HOMMA_TEST_SHOOK_INSTALL_IN_CHILD";

/// The real git-shook, named by `GIT_SHOOK`: the gate and the workspace's
/// manifest registered through it, run on a real commit and a real push.
///
/// The install writes the manifests in the process that calls it and the
/// commit then executes git-shook's dispatchers, so the install runs in a child
/// of this test binary that runs only this test: no thread of this process can
/// carry a descriptor of a file open for writing into a fork.
#[test]
fn the_gate_and_the_workspace_hooks_run_through_a_real_git_shook() {
    let Some(script) = std::env::var_os("GIT_SHOOK").map(PathBuf::from) else {
        eprintln!("skipped: GIT_SHOOK names no git-shook script");
        return;
    };
    let shook_dir = script.parent().unwrap().to_path_buf();
    if let Some(root) = std::env::var_os(INSTALL_IN_CHILD) {
        let root = PathBuf::from(root);
        let w = workspace_hooks(&root.join("ws"), Path::new("shook.toml"))
            .unwrap()
            .unwrap();
        let i = install_with(&root.join("member"), Some(&w), Some(&shook_dir)).unwrap();
        assert_eq!(i.registered, vec!["homma", "prose"]);
        return;
    }
    let top = tempfile::tempdir().unwrap();
    let ws = top.path().join("ws");
    std::fs::create_dir(&ws).unwrap();
    std::fs::write(
        ws.join("shook.toml"),
        "name = \"prose\"\n\n[hooks.pre-commit]\nrun = [\"homma\", \"prose\", \"{paths}\"]\npaths = [\"*.md\"]\n",
    )
    .unwrap();
    let member = top.path().join("member");
    std::fs::create_dir(&member).unwrap();
    for args in [&["init", "--quiet", "-b", "main"][..], &["config", "user.name", "t"], &[
        "config",
        "user.email",
        "t@t",
    ]] {
        assert!(sh::run(&member, "git", args).unwrap().ok());
    }
    // git-shook takes the hooks path first, as the install asks of a clone
    let path = format!(
        "{}:{}",
        shook_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let manifests = manifests_dir(&member).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    std::fs::write(
        manifests.join("first.toml"),
        "name = \"first\"\n\n[hooks.post-commit]\nrun = [\"true\"]\n",
    )
    .unwrap();
    let took = sh::run_with_env(&member, "git", &["shook", "install", "--yes"], &[(
        "PATH", &path,
    )])
    .unwrap();
    assert!(took.ok(), "{}", took.log());

    let installed = homma_fixture::alone(
        module_path!(),
        "the_gate_and_the_workspace_hooks_run_through_a_real_git_shook",
    )
    .env(INSTALL_IN_CHILD, top.path())
    .output()
    .unwrap();
    homma_fixture::passed(&installed);

    // a homma that records how it was called, ahead of any on the machine
    let bin = tempfile::tempdir().unwrap();
    let log = bin.path().join("called");
    executable(
        bin.path(),
        "homma",
        &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    );
    let path = format!("{}:{path}", bin.path().display());
    std::fs::write(member.join("notes.md"), "x").unwrap();
    std::fs::write(member.join("code.rs"), "x").unwrap();
    assert!(
        sh::run_with_env(&member, "git", &["add", "."], &[("PATH", &path)])
            .unwrap()
            .ok()
    );
    let committed = sh::run_with_env(&member, "git", &["commit", "-qm", "one"], &[(
        "PATH", &path,
    )])
    .unwrap();
    assert!(committed.ok(), "{}", committed.log());
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        "prose notes.md",
        "the workspace's entry ran for the markdown it selected and nothing else"
    );

    let remote = top.path().join("remote.git");
    assert!(
        sh::run(top.path(), "git", &[
            "init",
            "--quiet",
            "--bare",
            remote.to_str().unwrap()
        ])
        .unwrap()
        .ok()
    );
    let pushed = sh::run_with_env(
        &member,
        "git",
        &["push", "--quiet", remote.to_str().unwrap(), "main"],
        &[("PATH", &path)],
    )
    .unwrap();
    assert!(pushed.ok(), "{}", pushed.log());
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(
        calls.lines().any(|l| l.starts_with("release gate --hook")),
        "the push ran the release gate: {calls}"
    );
}

#[test]
fn a_refusal_puts_back_what_an_older_homma_left() {
    let d = held();
    let bin = tempfile::tempdir().unwrap();
    executable(
        bin.path(),
        "git-shook",
        "#!/bin/sh\necho refused >&2\nexit 1\n",
    );
    let own = common_dir(d.path()).unwrap().join("hooks");
    std::fs::create_dir_all(&own).unwrap();
    let left = own.join("pre-push");
    homma_fixture::script(&left, &legacy::entrypoint("pre-push"), 0o755);

    assert!(install_with(d.path(), None, Some(bin.path())).is_err());

    assert_eq!(
        std::fs::read_to_string(&left).unwrap(),
        legacy::entrypoint("pre-push"),
        "the old gate is back where git reads it"
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&left).unwrap().permissions().mode() & 0o777,
        0o755
    );
}
