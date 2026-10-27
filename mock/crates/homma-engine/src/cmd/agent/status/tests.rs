//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

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
fn the_json_has_a_probe_per_asked_surface_and_none_for_the_two_dropped() {
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
    // A repo that carries both surfaces still has no field for either: the
    // control that the absence is the document's shape, not the probe's value.
    let without = serde_json::to_value(probe("r", &repo)).unwrap();
    for key in [
        "mockspace_toml",
        "mock_dir",
        "mock_dir_name",
        "launcher_pin",
        "claude_dir",
        "github_instructions",
        "git_hooks_path",
    ] {
        assert!(
            without["surfaces"].get(key).is_some(),
            "{key} left the document"
        );
    }
    for key in ["mock_agent_dir", "cargo_mock_alias"] {
        assert!(
            without["surfaces"].get(key).is_none(),
            "{key} is back in the document"
        );
    }
}

#[test]
fn not_configured_when_there_is_nothing() {
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

#[test]
fn every_surface_is_asked_of_an_adopted_repo_on_its_own() {
    // The roll-up is a conjunction. With everything else present and one
    // surface absent the repo is partial, for each surface in turn, so a
    // surface dropped from the conjunction is caught by its own case.
    for absent in ["mock dir", ".claude", ".github/instructions", "git hooks"] {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("r");
        fs::create_dir_all(&repo).unwrap();
        every_surface_but_hooks(&repo);
        if absent != "git hooks" {
            wire_the_hooks(&repo);
        }
        match absent {
            "mock dir" => fs::remove_dir_all(repo.join("mock")).unwrap(),
            ".claude" => fs::remove_dir_all(repo.join(".claude")).unwrap(),
            ".github/instructions" => fs::remove_dir_all(repo.join(".github")).unwrap(),
            _ => {},
        }
        let state = probe("r", &repo);
        assert!(
            matches!(state.state, AgentState::Partial),
            "without {absent} the repo reported as {:?}",
            state.state
        );
    }
}

#[test]
fn a_toml_that_does_not_parse_still_counts_as_adopting_and_names_the_default_dir() {
    // The status says what it can see. A toml it cannot parse is still a toml,
    // so the repo has adopted mockspace, and the mock directory is the default
    // rather than a guess.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = [unclosed\n").unwrap();
    let state = probe("r", &repo);
    assert!(state.surfaces.mockspace_toml);
    assert_eq!(state.surfaces.mock_dir_name, "mock");
}

#[test]
fn the_launcher_pin_is_a_top_level_key_of_the_toml_and_nothing_else() {
    let pinned = |toml: &str| -> bool {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mockspace.toml"), toml).unwrap();
        probe("r", dir.path()).surfaces.launcher_pin
    };
    for key in ["mockspace_version", "mockspace_branch", "mockspace_rev", "mockspace_tag"] {
        assert!(pinned(&format!("{key} = \"x\"\n")), "{key} is a pin");
    }
    // The controls: no pin, a pin-looking key under a table, and a toml that
    // does not parse, which carries none.
    assert!(!pinned("mock_dir = \"mock\"\n"));
    assert!(!pinned("[t]\nmockspace_branch = \"dev\"\n"));
    assert!(!pinned("mockspace_branch = \n"));
    // The launcher reads a pin only where its value is a string.
    assert!(!pinned("mockspace_branch = 3\n"));
    assert!(!pinned("mockspace_rev = true\n"));
}

#[test]
fn a_nested_array_before_the_mock_dir_does_not_hide_it() {
    // A valid toml the gate's awk once read the default from: an element line
    // of a multi-line array begins with `[`.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("linux")).unwrap();
    fs::write(
        repo.join("mockspace.toml"),
        "layers = [\n  [\"a\", \"b\"],\n]\nmock_dir = \"linux\"\n",
    )
    .unwrap();
    let state = probe("r", &repo);
    assert_eq!(state.surfaces.mock_dir_name, "linux");
    assert!(state.surfaces.mock_dir);
}

#[test]
fn the_root_toml_wins_over_the_one_under_the_mock_dir() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r");
    fs::create_dir_all(repo.join("mock")).unwrap();
    fs::write(repo.join("mockspace.toml"), b"mock_dir = \"root\"\n").unwrap();
    fs::write(repo.join("mock/mockspace.toml"), b"mock_dir = \"under\"\n").unwrap();
    assert_eq!(probe("r", &repo).surfaces.mock_dir_name, "root");

    // The control: with only the one under `mock/`, that one is read.
    let only = dir.path().join("s");
    fs::create_dir_all(only.join("mock")).unwrap();
    fs::write(only.join("mock/mockspace.toml"), b"mock_dir = \"under\"\n").unwrap();
    assert_eq!(probe("s", &only).surfaces.mock_dir_name, "under");
}

#[test]
fn a_mock_dir_is_read_as_the_toml_reads_it() {
    // The two spellings the gate's awk once handled by hand: a literal string,
    // and a key under an indented table, which is not the top-level one.
    let dir = tempfile::tempdir().unwrap();
    let literal = dir.path().join("a");
    fs::create_dir_all(literal.join("linux")).unwrap();
    fs::write(literal.join("mockspace.toml"), "mock_dir = 'linux'\n").unwrap();
    assert_eq!(probe("a", &literal).surfaces.mock_dir_name, "linux");

    let indented = dir.path().join("b");
    fs::create_dir_all(&indented).unwrap();
    fs::write(
        indented.join("mockspace.toml"),
        "mockspace_branch = \"dev\"\n  [t]\n  mock_dir = \"elsewhere\"\n",
    )
    .unwrap();
    assert_eq!(probe("b", &indented).surfaces.mock_dir_name, "mock");
}
