use std::fs;

use super::*;

/// A repo with every surface but the hooks wiring.
///
/// The wiring cannot be faked by writing files, because it is a git
/// config setting, so a test wanting it present makes a real
/// repository and sets it.
fn every_surface_but_hooks(repo: &Path) {
    fs::create_dir_all(repo.join("mock")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"mock\"\n").unwrap();
    fs::create_dir_all(repo.join(".claude")).unwrap();
    fs::create_dir_all(repo.join(".github/instructions")).unwrap();
}

fn wire_the_hooks(repo: &Path) {
    homma_core::testing::init_repo(repo);
    homma_core::testing::set_hooks_path(repo, "mock/target/hooks");
}

#[test]
fn configured_when_all_surfaces_present_including_the_hooks_wiring() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    every_surface_but_hooks(&repo);
    wire_the_hooks(&repo);
    let state = probe("r", &repo);
    assert!(matches!(state.state, AgentState::Configured));
    assert!(state.surfaces.git_hooks_path);
}

#[test]
fn a_repo_with_every_surface_but_no_hooks_wiring_is_partial() {
    // The state that went unreported: the files are all there, so the
    // tree looks adopted, and git invokes none of the hooks. It read as
    // configured until the wiring became a surface.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    every_surface_but_hooks(&repo);
    let state = probe("r", &repo);
    assert!(!state.surfaces.git_hooks_path);
    assert!(
        matches!(state.state, AgentState::Partial),
        "a repo whose hooks never fire reported as {:?}",
        state.state
    );
}

#[test]
fn partial_when_adopted_but_render_missing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock/agent")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"mock\"\n").unwrap();
    let state = probe("r", &repo);
    assert!(matches!(state.state, AgentState::Partial));
}

#[test]
fn a_mock_dir_with_no_toml_has_no_mockspace() {
    // mockspace's own tree: a mock directory, and `cargo mock` answering
    // "no mockspace.toml found" there. Adoption is the toml, so it is not
    // asked for surfaces it cannot render.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock/agent")).unwrap();
    let state = probe("r", &repo);
    assert!(state.surfaces.mock_dir);
    assert!(!state.surfaces.mockspace_toml);
    assert!(matches!(state.state, AgentState::NotConfigured));
}

#[test]
fn the_json_keeps_its_deprecated_fields_with_their_probe_values() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock/agent")).unwrap();
    fs::create_dir_all(repo.join(".cargo")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"mock\"\n").unwrap();
    fs::write(
        repo.join(".cargo/config.toml"),
        b"[alias]\nmock = \"run\"\n",
    )
    .unwrap();
    let with = serde_json::to_value(probe("r", &repo)).unwrap();
    assert_eq!(with["surfaces"]["mock_agent_dir"], true);
    assert_eq!(with["surfaces"]["cargo_mock_alias"], true);

    let bare = tempfile::tempdir().unwrap();
    fs::write(bare.path().join("mockspace.toml"), b"mock_dir = \"mock\"\n").unwrap();
    let without = serde_json::to_value(probe("r", bare.path())).unwrap();
    for key in [
        "mockspace_toml",
        "mock_dir",
        "mock_dir_name",
        "claude_dir",
        "github_instructions",
        "git_hooks_path",
    ] {
        assert!(
            without["surfaces"].get(key).is_some(),
            "{key} left the document"
        );
    }
    assert_eq!(without["surfaces"]["mock_agent_dir"], false);
    assert_eq!(without["surfaces"]["cargo_mock_alias"], false);
}

#[test]
fn not_configured_when_mock_absent() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    let state = probe("r", &repo);
    assert!(matches!(state.state, AgentState::NotConfigured));
}

#[test]
fn missing_when_local_path_absent() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("does-not-exist");
    let state = probe("r", &repo);
    assert!(matches!(state.state, AgentState::Missing));
}

#[test]
fn a_launcher_repo_has_no_cargo_alias_and_is_not_asked_for_one() {
    // The launcher is installed once per machine and answers `cargo
    // mock` itself, so a repo carries no `.cargo` alias. A repo with
    // every rendered surface and no alias is the ordinary finished
    // state, and it read as partial for as long as the alias counted.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    every_surface_but_hooks(&repo);
    wire_the_hooks(&repo);
    assert!(!repo.join(".cargo").exists());
    let state = probe("r", &repo);
    assert!(
        matches!(state.state, AgentState::Configured),
        "a repo without the retired alias reported as {:?}",
        state.state
    );
}

#[test]
fn a_repo_with_no_agent_templates_of_its_own_is_not_partial() {
    // The builtin rules ship inside mockspace and are written into
    // every consumer, so `mock/agent/` holds only what a repo adds.
    // homma and muisti have none and are fully rendered.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    every_surface_but_hooks(&repo);
    wire_the_hooks(&repo);
    assert!(!repo.join("mock/agent").exists());
    assert!(matches!(probe("r", &repo).state, AgentState::Configured));
}

#[test]
fn the_mock_dir_is_the_one_mockspace_toml_declares() {
    // `mock` is only the default: sodapop declares `linux`, and a probe
    // for a literal `mock/` reports a fully adopted repo as having
    // none.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("linux")).unwrap();
    fs::create_dir_all(repo.join(".claude")).unwrap();
    fs::create_dir_all(repo.join(".github/instructions")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"linux\"\n").unwrap();
    wire_the_hooks(&repo);
    let state = probe("r", &repo);
    assert!(state.surfaces.mock_dir);
    assert!(matches!(state.state, AgentState::Configured));
}

#[test]
fn a_declared_mock_dir_that_is_absent_is_named_by_its_declared_name() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"linux\"\n").unwrap();
    let state = probe("r", &repo);
    assert!(matches!(state.state, AgentState::Partial));
    assert!(!state.surfaces.mock_dir);
    assert_eq!(state.surfaces.mock_dir_name, "linux");
}

#[test]
fn a_repo_with_a_mockspace_toml_and_nothing_rendered_is_partial() {
    // The control on the not-configured cases: adoption by the config
    // alone must still be reported, or silence for the repos with no
    // mockspace could be a probe that never reports anything.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"mock\"\n").unwrap();
    assert!(matches!(probe("r", &repo).state, AgentState::Partial));
}

#[test]
fn a_mockspace_toml_under_the_mock_dir_counts_as_adopting() {
    // ikiuni keeps its config at `mock/mockspace.toml`, which the
    // commit gate already reads. A probe that looks only at the root
    // reports it as having no mockspace, and says nothing of what it
    // lacks.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock")).unwrap();
    fs::write(repo.join("mock/mockspace.toml"), b"project_name = \"r\"\n").unwrap();
    let state = probe("r", &repo);
    assert!(state.surfaces.mockspace_toml);
    assert!(matches!(state.state, AgentState::Partial));
}

#[test]
fn a_repo_with_a_cargo_manifest_and_no_mockspace_is_not_configured() {
    // notko, git-shook and the bash libraries are built without
    // mockspace on purpose, so the absence is a fact about the repo and
    // not a gap in it.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("Cargo.toml"), b"[package]\nname = \"r\"\n").unwrap();
    assert!(matches!(probe("r", &repo).state, AgentState::NotConfigured));
}
