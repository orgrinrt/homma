//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

#[test]
fn last_meaningful_line_picks_last_nonempty() {
    let s = "first\nsecond\n\n   \nthird\n";
    assert_eq!(last_meaningful_line(s), Some("third"));
}

#[test]
fn last_meaningful_line_empty_input_is_none() {
    assert_eq!(last_meaningful_line(""), None);
    assert_eq!(last_meaningful_line("\n\n   \n"), None);
}

#[test]
fn truncate_short_is_unchanged() {
    assert_eq!(truncate("hello".into(), 16), "hello");
}

#[test]
fn truncate_long_clipped_with_ellipsis() {
    let s = "abcdefghij".to_string();
    let out = truncate(s, 5);
    assert_eq!(out, "abcde...");
}

#[test]
fn truncate_walks_back_to_char_boundary() {
    // Two-byte chars; cutting at 3 lands mid-char.
    let s = "ééé".to_string(); // 6 bytes
    let out = truncate(s, 3);
    // Walks back to byte 2 (between first and second char).
    assert!(out.ends_with("..."));
    assert!(out.is_char_boundary(out.len() - 3));
}

/// A workspace of temporary directories holding one member, `r`, so the
/// stage under test is reached through `regen` and not through a helper.
fn workspace_with_member(ws: &std::path::Path) -> homma_core::Config {
    let mut cfg = homma_core::Config::parse(&format!(
        "content_repo = \"c\"\n[workspace]\nname = \"w\"\npath = {:?}\n",
        ws.display().to_string()
    ))
    .unwrap();
    cfg.repos
        .insert("r".into(), homma_core::config::RepoConfig {
            forge:      None,
            owner:      None,
            local_path: "r".into(),
        });
    cfg
}

fn cargo_mock_stage(cfg: &homma_core::Config) -> StageStatus {
    let opts = Opts {
        skip_configs: true,
        skip_aggregate: true,
        ..Opts::default()
    };
    regen(cfg, Some("r"), opts)
        .unwrap()
        .results
        .remove(0)
        .cargo_mock
}

#[test]
fn a_mock_dir_with_no_toml_is_skipped_and_cargo_mock_is_not_run() {
    // mockspace's own tree: `mock/` and no `mockspace.toml`. Skipping keys on
    // the toml, so `cargo mock` is never invoked there.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("r/mock")).unwrap();
    let cfg = workspace_with_member(dir.path());
    assert!(
        matches!(cargo_mock_stage(&cfg), StageStatus::Skipped(ref why) if why == "no mockspace.toml"),
        "got {:?}",
        cargo_mock_stage(&cfg)
    );
}

#[test]
fn a_repo_with_a_toml_and_a_declared_mock_dir_is_not_skipped() {
    // The control: shingle's `linux/` with its toml reaches `cargo mock`, so
    // the test above does not pass for a stage that skips everything. What
    // `cargo mock` then answers in a temporary directory is not the question.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("r/linux")).unwrap();
    std::fs::write(
        dir.path().join("r/mockspace.toml"),
        b"mock_dir = \"linux\"\n",
    )
    .unwrap();
    let cfg = workspace_with_member(dir.path());
    assert!(!matches!(cargo_mock_stage(&cfg), StageStatus::Skipped(_)));
}

#[test]
fn the_cargo_mock_command_carries_generate_only() {
    let cmd = cargo_mock_command("cargo", std::path::Path::new("."));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["mock", "--generate-only"]);
}

/// A stub launcher: renders (exit 0) only under `--generate-only`, and fails
/// the way a red `cargo check` does otherwise.
#[cfg(unix)]
fn stub_launcher(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("stub-cargo");
    homma_fixture::script(
        &p,
        "#!/bin/sh\ncase \"$*\" in *--generate-only*) touch rendered; exit 0;; esac\necho 'cargo check failed' >&2\nexit 1\n",
        0o755,
    );
    p
}

#[cfg(unix)]
#[test]
fn a_member_whose_check_would_fail_is_still_rendered() {
    let dir = tempfile::tempdir().unwrap();
    let member = tempfile::tempdir().unwrap();
    let stub = stub_launcher(dir.path());
    invoke_cargo_mock_with(stub.to_str().unwrap(), member.path()).unwrap();
    assert!(member.path().join("rendered").exists());
}

/// Only the surfaces stage runs: the other three are skipped, so what the
/// report says about the surfaces is the stage under test and nothing else.
fn surfaces_only(skip_surfaces: bool) -> Opts {
    Opts {
        skip_cargo_mock: true,
        skip_configs: true,
        skip_aggregate: true,
        skip_surfaces,
        ..Opts::default()
    }
}

/// A workspace whose `[paths]` names `bin/render` when `body` is given, with
/// that file a script of `body`.
#[cfg(unix)]
fn workspace_with_renderer(ws: &std::path::Path, body: Option<&str>) -> homma_core::Config {
    let mut text = format!(
        "content_repo = \"c\"\n[workspace]\nname = \"w\"\npath = {:?}\n",
        ws.display().to_string()
    );
    if let Some(body) = body {
        text.push_str("[paths]\nagent_surfaces = \"bin/render\"\n");
        homma_fixture::script(
            &ws.join("bin/render"),
            &format!("#!/bin/sh\n{body}\n"),
            0o755,
        );
    }
    homma_core::Config::parse(&text).unwrap()
}

/// Records its arguments and where it ran, as the renderer would render.
const RECORDING: &str = "printf '%s\\n%s\\n' \"$*\" \"$(pwd -P)\" > ran";

#[cfg(unix)]
#[test]
fn the_configured_renderer_is_run_as_regen_from_the_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_renderer(dir.path(), Some(RECORDING));
    let report = regen(&cfg, None, surfaces_only(false)).unwrap();
    assert!(
        matches!(report.surfaces, StageStatus::Success),
        "got {:?}",
        report.surfaces
    );
    assert!(report.ok);
    let ran = std::fs::read_to_string(dir.path().join("ran")).expect("the renderer was not run");
    let root = dir.path().canonicalize().unwrap();
    assert_eq!(ran, format!("regen\n{}\n", root.display()));
}

#[cfg(unix)]
#[test]
fn skip_surfaces_leaves_the_configured_renderer_unrun() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_renderer(dir.path(), Some(RECORDING));
    // The configs stage is left running, so the all-skip guard does not
    // refuse the run and the flag alone is what skips the surfaces.
    let opts = Opts {
        skip_cargo_mock: true,
        skip_aggregate: true,
        skip_surfaces: true,
        ..Opts::default()
    };
    let report = regen(&cfg, None, opts).unwrap();
    assert!(
        matches!(report.surfaces, StageStatus::Skipped(ref why) if why == "--skip-surfaces"),
        "got {:?}",
        report.surfaces
    );
    assert!(
        !dir.path().join("ran").exists(),
        "the renderer ran under --skip-surfaces"
    );
}

#[cfg(unix)]
#[test]
fn an_unset_key_skips_the_stage_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_renderer(dir.path(), None);
    let report = regen(&cfg, None, surfaces_only(false)).unwrap();
    assert!(
        matches!(report.surfaces, StageStatus::Skipped(ref why) if why.contains("agent_surfaces")),
        "got {:?}",
        report.surfaces
    );
    assert!(report.ok, "an unset key is not a failure");
}

#[cfg(unix)]
#[test]
fn a_failed_render_fails_the_run_with_its_last_line() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_renderer(
        dir.path(),
        Some("echo 'homma rules render failed' >&2; exit 3"),
    );
    let report = regen(&cfg, None, surfaces_only(false)).unwrap();
    assert!(
        matches!(report.surfaces, StageStatus::Failed(ref m) if m.contains("homma rules render failed")),
        "got {:?}",
        report.surfaces
    );
    assert!(!report.ok, "a failed render did not fail the run");
}

#[cfg(unix)]
#[test]
fn a_key_naming_nothing_on_disk_is_a_failure_not_a_skip() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_renderer(dir.path(), Some(RECORDING));
    std::fs::remove_file(dir.path().join("bin/render")).unwrap();
    let report = regen(&cfg, None, surfaces_only(false)).unwrap();
    assert!(
        matches!(report.surfaces, StageStatus::Failed(ref m) if m.contains("which is not there")),
        "got {:?}",
        report.surfaces
    );
    assert!(!report.ok);
}

#[test]
fn skipping_all_four_stages_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = workspace_with_member(dir.path());
    let err = regen(&cfg, None, surfaces_only(true)).unwrap_err();
    assert!(err.to_string().contains("would do nothing"), "got {err}");
}

#[test]
fn an_agent_entry_this_engine_does_not_read_stops_the_regen_and_says_what_to_do() {
    let skip_all = Opts {
        skip_cargo_mock: true,
        skip_configs: true,
        skip_aggregate: true,
        skip_surfaces: true,
        ..Opts::default()
    };
    let later = homma_core::Config::parse(
        "homma_branch = \"dev\"\n[workspace]\nname = \"w\"\n[[agent.later]]\na = 1\n",
    )
    .unwrap();
    let why = format!("{:#}", regen(&later, None, skip_all).unwrap_err());
    assert!(why.contains("`agent.later`"), "{why}");
    assert!(why.contains("the branch `dev`"), "{why}");
    // Failing control: an entry of the top level alone is read by no regen, so whatever the
    // regen says of a run that skips every stage, it is not this.
    let top = homma_core::Config::parse(
        "homma_branch = \"dev\"\n[workspace]\nname = \"w\"\n[later]\nx = 1\n",
    )
    .unwrap();
    if let Err(e) = regen(&top, None, skip_all) {
        assert!(!format!("{e:#}").contains("does not read"), "{e:#}");
    }
}
