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
    // The control: sodapop's `linux/` with its toml reaches `cargo mock`, so
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
