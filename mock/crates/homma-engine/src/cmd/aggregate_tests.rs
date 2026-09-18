//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The pass over one repository, and which registrations count as homma's.

use super::*;

/// A `Root` over a test workspace, denying nothing that a test uses.
///
/// The real code path, not a variant of it: these go through
/// `Root::contain` exactly as production does, which is what makes the
/// containment they assert mean anything.
pub(crate) fn test_root(workspace: &Path) -> Root {
    Root::new(
        &homma_api::AbsPath::new(workspace).expect("a tempdir path is absolute"),
        homma_api::Denied::under_home(&homma_api::AbsPath::new("/nonexistent-home").unwrap()),
    )
    .expect("a tempdir is a legitimate root")
}

/// Mark a file executable, which the wrapper's own `[ -x ]` check reads.
pub(crate) fn make_executable(p: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(p).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(p, perm).unwrap();
    }
}

#[test]
fn aggregated_command_detected_by_prefix() {
    let cmd = ".claude/hooks/arvo--no-alloc-guard.sh";
    assert!(is_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
}

#[test]
fn non_aggregated_command_not_detected() {
    let cmd = ".claude/hooks/workspace-only.sh";
    assert!(!is_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn legacy_imports_path_detected_as_aggregated() {
    // Pre-homma bash aggregator wrote per-repo hooks under
    // `imports/<repo>/<name>.sh` rather than the current flat
    // `<repo>--<name>.sh` convention. Entries left over from that
    // era must still get swept out on regen.
    let cmd = ".claude/hooks/imports/arvo/no-alloc-guard.sh";
    assert!(is_retired_aggregated_command(cmd, &[
        "arvo",
        "hilavitkutin"
    ]));
    // and the current-shape check does not claim it, so the two are not
    // silently the same predicate under two names
    assert!(!is_aggregated_command(cmd, &["arvo", "hilavitkutin"]));
}

#[test]
fn legacy_imports_at_path_start_detected_as_aggregated() {
    // Relative path starting with `imports/<repo>/` (no leading
    // separator). Must still match the legacy pattern.
    let cmd = "imports/arvo/no-alloc-guard.sh";
    assert!(is_retired_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn an_absolute_managed_command_is_retired_whatever_repo_it_names() {
    let cmd = "/Users/someone/Dev/their-workspace/.claude/hooks/arvo--no-alloc-guard.sh";
    assert!(is_retired_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn a_placeholder_command_for_an_unvisited_repo_is_not_retired() {
    // The control that keeps the preservation rule alive. A predicate that
    // swept both shapes would satisfy the test above and quietly undo it.
    let cmd = "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--no-alloc-guard.sh";
    assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn an_unmanaged_absolute_command_is_left_alone() {
    // The second control, and the worst thing this could get wrong: a
    // hand-authored user-level hook is absolute too.
    let cmd = "/Users/someone/.claude/hooks/self-compact-trigger.sh";
    assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
    assert!(!is_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn imports_substring_not_at_path_boundary_not_detected() {
    // A command that happens to contain the substring `imports/arvo/`
    // in the middle of a longer path component must NOT be flagged.
    // Path-component anchoring prevents false positives on
    // e.g. user-authored paths like `myimports/arvo/foo.sh`.
    let cmd = ".claude/hooks/myimports/arvo/foo.sh";
    assert!(!is_retired_aggregated_command(cmd, &["arvo"]));
}

#[test]
fn matcher_detection_from_hook_body_directive() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("hook.sh");
    fs::write(
        &p,
        "#!/usr/bin/env bash\n# @matchers: Write, Edit\n# Some hook.\n",
    )
    .unwrap();
    let m = detect_matchers_from_hook_body(&p).unwrap();
    assert_eq!(m, vec!["Write".to_string(), "Edit".to_string()]);
}

#[test]
fn aggregate_repo_end_to_end_against_synthetic_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path();
    let repo_abs = workspace.join("arvo");
    fs::create_dir_all(repo_abs.join(".claude/rules")).unwrap();
    fs::create_dir_all(repo_abs.join(".claude/hooks")).unwrap();

    // Stale aggregated rule from a prior homma version: must get
    // swept out on regen even though no new rule is being written.
    fs::create_dir_all(workspace.join(".claude/rules")).unwrap();
    fs::write(
        workspace.join(".claude/rules/arvo--stale-rule.md"),
        "---\npaths:\n  - \"arvo/**\"\n---\nLeftover.\n",
    )
    .unwrap();

    // Per-repo source rules still live in the repo; homma no longer
    // copies them. Verify by writing one and checking it does NOT
    // appear in the workspace .claude/rules/ post-aggregate.
    fs::write(
        repo_abs.join(".claude/rules/type-surface.md"),
        "---\npaths:\n  - \"crates/**/*.rs\"\n---\nBody.\n",
    )
    .unwrap();
    fs::write(
        repo_abs.join(".claude/hooks/no-alloc.sh"),
        "#!/usr/bin/env bash\n# @matchers: Write, Edit\necho hi\n",
    )
    .unwrap();
    make_executable(&repo_abs.join(".claude/hooks/no-alloc.sh"));
    fs::write(
        repo_abs.join(".claude/settings.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":".claude/hooks/no-alloc.sh"}]}]}}"#,
    )
    .unwrap();

    let mut settings = Vec::new();
    let a = aggregate_repo(&test_root(workspace), "arvo", &repo_abs, &mut settings).unwrap();
    assert_eq!(a.hooks, 1);
    assert!(a.problems.is_empty(), "{:?}", a.problems);

    // Stale aggregated rule was cleaned.
    assert!(
        !workspace.join(".claude/rules/arvo--stale-rule.md").exists(),
        "stale aggregated rule should have been cleaned by clean_stale"
    );
    // Repo-side rule was NOT propagated.
    assert!(
        !workspace
            .join(".claude/rules/arvo--type-surface.md")
            .exists(),
        "homma no longer aggregates per-repo rules"
    );

    let hook = fs::read_to_string(workspace.join(".claude/hooks/arvo--no-alloc.sh")).unwrap();
    assert!(hook.contains("REPO_REL='arvo'"));
    assert!(hook.contains("ORIG_HOOK="));
    assert!(
        !hook.contains(workspace.to_str().unwrap()),
        "the wrapper baked the generating workspace's path"
    );

    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].event, "PreToolUse");
    assert_eq!(settings[0].matcher, "Edit");
    assert_eq!(
        settings[0].command, "\"${CLAUDE_PROJECT_DIR}\"/.claude/hooks/arvo--no-alloc.sh",
        "the registered command must name the host's project-root placeholder rather \
         than this run's workspace",
    );
    assert!(
        !settings[0].command.contains(workspace.to_str().unwrap()),
        "expected no generating-workspace path in the command, got: {}",
        settings[0].command,
    );

    merge_settings(&test_root(workspace), &["arvo"], &["arvo"], &settings, None).unwrap();
    let written = fs::read_to_string(workspace.join(".claude/settings.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&written).unwrap();
    let arr = v["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["matcher"], "Edit");
    let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(
        cmd.ends_with("/.claude/hooks/arvo--no-alloc.sh"),
        "expected absolute path, got: {cmd}",
    );
}
