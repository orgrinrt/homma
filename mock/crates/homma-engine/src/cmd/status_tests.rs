//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::cmd::agent::status::{AgentState, Surfaces};
use crate::cmd::config::RepoConfigState;

fn block(title: &str, text: &str, failed: Option<&str>) -> Injected {
    Injected {
        title:  title.into(),
        text:   text.into(),
        failed: failed.map(str::to_string),
    }
}

fn empty_configs() -> ConfigReport {
    ConfigReport {
        repos:      Vec::new(),
        unreadable: None,
        blocked:    false,
    }
}

fn report(injected: Vec<Injected>) -> StatusReport {
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
    build_report(&cfg, injected, Vec::new(), empty_configs(), Vec::new())
}

fn with(
    agent: Vec<RepoAgentState>,
    configs: ConfigReport,
    worktrees: Vec<WorktreeLine>,
) -> StatusReport {
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
    build_report(&cfg, Vec::new(), agent, configs, worktrees)
}

fn surfaces(all: bool) -> Surfaces {
    Surfaces {
        mockspace_toml:      true,
        mock_dir_name:       "mock".into(),
        mock_dir:            true,
        launcher_pin:        false,
        claude_dir:          true,
        github_instructions: true,
        git_hooks_path:      all,
    }
}

fn agent_state(repo: &str, state: AgentState, all: bool) -> RepoAgentState {
    RepoAgentState {
        repo: repo.into(),
        local_path: format!("/ws/{repo}"),
        state,
        surfaces: surfaces(all),
    }
}

fn rendered(report: &StatusReport) -> String {
    let mut out = Vec::new();
    report
        .render_human(&mut out)
        .expect("writing to a vec cannot fail");
    String::from_utf8(out).expect("the render is utf-8")
}

fn rendered_full(report: &StatusReport) -> String {
    let mut out = Vec::new();
    report
        .render(&mut out, true)
        .expect("writing to a vec cannot fail");
    String::from_utf8(out).expect("the render is utf-8")
}

/// The report over a real file read from a real directory, the way `run`
/// is handed it.
fn with_local(local: &str) -> StatusReport {
    let d = tempfile::tempdir().expect("a temp dir");
    std::fs::write(d.path().join(homma_core::local::LOCAL_FILE), local).expect("writable");
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
    let mut r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
    r.carry_local(Local::load(d.path()));
    r
}

#[test]
fn a_scoped_name_with_no_clone_is_reported_as_not_cloned() {
    let mut cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a manifest parses");
    cfg.scope = homma_core::scope::Scope::named(["kaski", "letkajenkka"]);
    cfg.repos.insert("kaski".into(), homma_core::RepoConfig {
        forge:      None,
        owner:      None,
        local_path: "kaski".into(),
    });
    let r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
    assert_eq!(r.not_cloned, ["letkajenkka"]);
    assert!(rendered(&r).contains("in scope and not cloned: letkajenkka"));
}

#[test]
fn no_scope_reports_nothing_as_not_cloned() {
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a manifest parses");
    let r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
    assert!(r.not_cloned.is_empty());
}

#[test]
fn a_broken_local_file_is_reported_rather_than_refused_or_ignored() {
    // A missing key, bad syntax, and an unknown field. Each leaves the
    // instance null, names the file, and carries the parser's own detail.
    for (broken, detail) in [
        ("[instance]\n", "work"),
        ("[instance\n", "line 1"),
        ("[instance]\nwork = \"k\"\nwhat = 1\n", "what"),
    ] {
        let r = with_local(broken);
        assert!(r.instance.is_none(), "{broken:?}");
        let e = r.instance_error.as_deref().expect("the error is carried");
        assert!(e.contains(homma_core::local::LOCAL_FILE), "{e}");
        assert!(e.contains(detail), "{broken:?} lost its detail: {e}");
        let human = rendered(&r);
        assert!(
            human.contains("what this clone is for is unknown: "),
            "{human}"
        );
        let doc = serde_json::to_value(&r).unwrap();
        assert!(doc["instance"].is_null(), "{doc}");
        assert_eq!(doc["instance_error"], e);
    }
}

#[test]
fn a_good_or_absent_local_file_carries_no_error() {
    let r = with_local("[instance]\nwork = \"k\"\n");
    assert!(r.instance_error.is_none());
    assert!(!rendered(&r).contains("unknown"));
    assert!(serde_json::to_value(&r).unwrap()["instance_error"].is_null());
    let r = report(Vec::new());
    assert!(r.instance_error.is_none());
}

#[test]
fn a_clone_without_a_local_file_says_nothing_about_what_it_is_for() {
    let r = report(Vec::new());
    assert!(r.instance.is_none());
    assert!(!rendered(&r).contains("this clone is for"));
    let doc = serde_json::to_value(&r).unwrap();
    assert!(doc["instance"].is_null(), "{doc}");
}

#[test]
fn a_clone_with_a_local_file_says_what_it_is_for() {
    let r = with_local("[instance]\nwork = \"kenno\"\n");
    assert!(
        rendered(&r).contains("  this clone is for kenno\n"),
        "{}",
        rendered(&r)
    );
    let r = with_local("[instance]\nwork = \"kenno\"\ntitle = \"the tile machine\"\n");
    assert!(
        rendered(&r).contains("  this clone is for kenno (the tile machine)\n"),
        "{}",
        rendered(&r)
    );
}

/// The report for a workspace holding the stored `k` and the given local file.
fn inheriting(local: &str) -> (StatusReport, String) {
    let d = tempfile::tempdir().expect("a temp dir");
    let store = d.path().join(homma_core::local::STORE_DIR);
    std::fs::create_dir_all(&store).expect("writable");
    std::fs::write(
        store.join("k.toml"),
        "[instance]\nwork = \"k\"\ntitle = \"stored\"\n[tools.a]\nx = \"1\"\ny = \"2\"\n",
    )
    .expect("writable");
    std::fs::write(d.path().join(homma_core::local::LOCAL_FILE), local).expect("writable");
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
    let mut r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
    r.carry_local(Local::load(d.path()));
    r.note_copy(d.path());
    let mut out = Vec::new();
    r.render(&mut out, false).expect("renders");
    (r, String::from_utf8(out).expect("utf8"))
}

#[test]
fn an_inheriting_clone_says_what_it_inherits_and_what_it_sets() {
    let (r, text) = inheriting(
        "[instance]\nproject = \"k\"\nunset = [\"tools.a.y\"]\n[tools.a]\nx = \"9\"\n[tools.b]\nz = \"1\"\n",
    );
    assert!(
        text.contains("inherits k: 2 inherited, 1 set here, 1 overridden, 1 unset"),
        "{text}"
    );
    assert!(r.holds_copy_of.is_none());
    assert_eq!(r.instance.as_ref().map(|i| i.work.as_str()), Some("k"));
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(doc["project"]["project"], "k");
    assert_eq!(doc["project"]["overridden"][0], "tools.a.x");
    assert_eq!(doc["project"]["unset"][0], "tools.a.y");
}

#[test]
fn a_clone_holding_a_copy_is_told_to_install_and_one_inheriting_is_not() {
    let (r, text) = inheriting("[instance]\nwork = \"k\"\n");
    assert_eq!(r.holds_copy_of.as_deref(), Some("k"));
    assert!(text.contains("homma local install k"), "{text}");
    assert!(!text.contains("inherits"), "{text}");
    let (r, text) = inheriting("[instance]\nproject = \"k\"\n");
    assert!(
        r.holds_copy_of.is_none() && !text.contains("holds a copy"),
        "{text}"
    );
    // A work with no stored config is nobody's copy.
    let (r, _) = inheriting("[instance]\nwork = \"other\"\n");
    assert!(r.holds_copy_of.is_none());
}

#[test]
fn a_project_the_store_lacks_is_the_instance_error() {
    let (r, text) = inheriting("[instance]\nproject = \"gone\"\n");
    assert!(r.instance.is_none() && r.project.is_none());
    assert!(text.contains("`gone`"), "{text}");
}

#[test]
fn the_document_carries_the_instance_whole() {
    let r = with_local(
        "[instance]\nwork = \"k\"\nrepos = [\"a\"]\nstate = \"s.md\"\ngoal = \"g.md\"\n[tools.x]\ny = \"z\"\n",
    );
    let doc = serde_json::to_value(&r).unwrap();
    assert_eq!(doc["instance"]["work"], "k");
    assert_eq!(doc["instance"]["repos"][0], "a");
    assert_eq!(doc["instance"]["state"], "s.md");
    assert_eq!(doc["instance"]["goal"], "g.md");
    // The tools' tables are theirs and stay out of the status document.
    assert!(
        doc.get("tools").is_none() && doc["instance"].get("tools").is_none(),
        "{doc}"
    );
}

#[test]
fn a_block_prints_its_title_and_its_text_indented() {
    let text = rendered(&report(vec![block("context", "450733 of 1000000", None)]));
    assert!(text.contains("context:\n  450733 of 1000000\n"), "{text}");
}

#[test]
fn a_blank_line_inside_a_block_stays_blank() {
    // Two spaces on an otherwise empty line is trailing whitespace in
    // somebody's terminal and in every diff of a captured status.
    let text = rendered(&report(vec![block("t", "a\n\nb", None)]));
    assert!(text.contains("  a\n\n  b\n"), "{text:?}");
}

#[test]
fn a_failed_block_says_so_on_the_heading() {
    // On the heading rather than under it, so a block with nothing in it
    // does not read as a tool that had nothing to say.
    let text = rendered(&report(vec![block("agenda", "", Some("agenda exited 1"))]));
    assert!(text.contains("agenda: agenda exited 1\n"), "{text}");
    assert!(
        !text.contains("agenda:\n"),
        "the empty-body shape must not appear: {text}"
    );
}

#[test]
fn blocks_print_in_the_order_they_arrived() {
    let text = rendered(&report(vec![
        block("first", "1", None),
        block("second", "2", None),
        block("third", "3", None),
    ]));
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("missing {needle} in {text}"))
    };
    assert!(at("first:") < at("second:"));
    assert!(at("second:") < at("third:"));
}

#[test]
fn a_report_with_no_injections_prints_nothing_extra() {
    // The control. Every assertion above would hold against a render that
    // printed blocks unconditionally, and a workspace declaring none is
    // every workspace but one.
    let bare = rendered(&report(vec![]));
    let with = rendered(&report(vec![block("t", "x", None)]));
    assert!(!bare.contains("t:"), "{bare}");
    assert_eq!(with.len() - bare.len(), "\nt:\n  x\n".len(), "{with:?}");
}

#[test]
fn a_tool_with_nothing_to_say_prints_no_heading() {
    // The silent block is absent from the text; the failed one, whose text is
    // empty too, is the control and keeps its heading.
    let text = rendered(&report(vec![
        block("dashboard", "", None),
        block("blank", " \n", None),
        block("agenda", "", Some("exited 1")),
        block("context", "45%", None),
    ]));
    assert!(!text.contains("dashboard"), "{text}");
    assert!(!text.contains("blank"), "{text}");
    assert!(text.contains("agenda: exited 1"), "{text}");
    assert!(text.contains("context:\n  45%"), "{text}");
}

#[test]
fn the_blocks_reach_the_json_document_too() {
    let doc = serde_json::to_value(report(vec![block("context", "45%", None)]))
        .expect("the report serialises");
    let blocks = doc["injected"].as_array().expect("an injected array");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["title"], "context");
    assert_eq!(blocks[0]["text"], "45%");
    assert!(
        blocks[0].get("failed").is_none(),
        "a block that worked carries no failure key: {blocks:?}"
    );
}

#[test]
fn a_failed_block_carries_its_reason_into_the_json_too() {
    let doc = serde_json::to_value(report(vec![block("agenda", "", Some("exited 1"))]))
        .expect("the report serialises");
    assert_eq!(doc["injected"][0]["failed"], "exited 1");
}

#[test]
fn building_a_report_spawns_nothing() {
    // `build_report` is a pure function of what it is handed, which is what
    // lets it be tested at all. Every population is gathered in `run`.
    let cfg = Config::parse("[workspace]\nname = \"w\"\n").expect("a minimal manifest parses");
    let r = build_report(&cfg, Vec::new(), Vec::new(), empty_configs(), Vec::new());
    assert!(r.injected.is_empty());
    assert!(r.agent.is_empty());
    assert!(r.worktrees.is_empty());
}

#[test]
fn a_healthy_workspace_says_nothing_about_any_population() {
    // The whole default. Thirty healthy members must not produce thirty
    // blocks, or nobody reads the one line that matters.
    let text = rendered(&with(
        vec![agent_state("arvo", AgentState::Configured, true)],
        ConfigReport {
            repos:      vec![RepoConfigState {
                repo:       "arvo".into(),
                local_path: "/ws/arvo".into(),
                findings:   Vec::new(),
                blocked:    false,
            }],
            unreadable: None,
            blocked:    false,
        },
        vec![WorktreeLine {
            repo:    "arvo".into(),
            branch:  Some("dev".into()),
            clean:   true,
            changes: 0,
            unread:  None,
        }],
    ));
    assert!(!text.contains("agent surfaces:"), "{text}");
    assert!(!text.contains("shared configs:"), "{text}");
    assert!(!text.contains("working trees:"), "{text}");
}

#[test]
fn the_same_healthy_workspace_prints_every_population_under_full() {
    // The control on the case above: without it, the silence there could be
    // a renderer that never prints these at all.
    let text = rendered_full(&with(
        vec![agent_state("arvo", AgentState::Configured, true)],
        ConfigReport {
            repos:      vec![RepoConfigState {
                repo:       "arvo".into(),
                local_path: "/ws/arvo".into(),
                findings:   Vec::new(),
                blocked:    false,
            }],
            unreadable: None,
            blocked:    false,
        },
        vec![WorktreeLine {
            repo:    "arvo".into(),
            branch:  Some("dev".into()),
            clean:   true,
            changes: 0,
            unread:  None,
        }],
    ));
    assert!(text.contains("agent surfaces:"), "{text}");
    assert!(text.contains("shared configs:"), "{text}");
    assert!(text.contains("working trees:"), "{text}");
    assert!(text.contains("clean on dev"), "{text}");
}

#[test]
fn a_repo_with_no_mockspace_is_not_reported_in_the_summary() {
    // notko, git-shook and the bash libraries have no mockspace on purpose.
    // Listing them as missing four directories every session is a line
    // nobody can act on, which is what trains a reader past the lines that
    // can be acted on.
    let text = rendered(&with(
        vec![agent_state("git-shook", AgentState::NotConfigured, false)],
        empty_configs(),
        Vec::new(),
    ));
    assert!(!text.contains("git-shook"), "{text}");
    assert!(!text.contains("agent surfaces:"), "{text}");
}

#[test]
fn a_repo_with_no_mockspace_is_named_as_that_under_full() {
    // The control on the case above: the silence there could be a renderer
    // that drops every not-configured state, and this shows it is only
    // the summary that does.
    let text = rendered_full(&with(
        vec![agent_state("git-shook", AgentState::NotConfigured, false)],
        empty_configs(),
        Vec::new(),
    ));
    assert!(text.contains("git-shook no mockspace"), "{text}");
    assert!(!text.contains("missing"), "{text}");
}

#[test]
fn a_missing_surface_names_the_command_that_renders_it_and_the_declared_mock_dir() {
    let mut a = agent_state("sodapop", AgentState::Partial, true);
    a.surfaces.mock_dir_name = "linux".into();
    a.surfaces.mock_dir = false;
    a.surfaces.claude_dir = false;
    let text = rendered(&with(vec![a], empty_configs(), Vec::new()));
    assert!(text.contains("sodapop missing linux/, .claude/"), "{text}");
    assert!(!text.contains("mock/agent"), "{text}");
    assert!(!text.contains("cargo-mock-alias"), "{text}");
    assert!(text.contains("homma agent regen"), "{text}");
}

#[test]
fn a_repo_whose_hooks_are_not_wired_shows_up_without_full() {
    // The state that went unreported entirely. It has to reach the default
    // output or the fold has bought nothing.
    let text = rendered(&with(
        vec![agent_state("kamu", AgentState::Partial, false)],
        empty_configs(),
        Vec::new(),
    ));
    assert!(text.contains("agent surfaces:"), "{text}");
    assert!(text.contains("kamu missing git-hooks"), "{text}");
    assert!(text.contains("run `homma agent regen`"), "{text}");
}

#[test]
fn a_repo_not_on_disk_is_not_told_to_regenerate() {
    // `regen` skips a path that does not exist, so the line naming it
    // would send somebody to a command that does nothing for that repo,
    // and the surfaces probed in a path that is not there say nothing.
    let text = rendered(&with(
        vec![agent_state("kamu", AgentState::Missing, false)],
        empty_configs(),
        Vec::new(),
    ));
    assert!(text.contains("kamu not on disk"), "{text}");
    assert!(!text.contains("kamu missing"), "{text}");
    assert!(!text.contains("homma agent regen"), "{text}");
}

#[test]
fn a_missing_config_shows_up_without_full_and_names_the_fix() {
    let text = rendered(&with(
        Vec::new(),
        ConfigReport {
            repos:      vec![RepoConfigState {
                repo:       "tassu".into(),
                local_path: "/ws/tassu".into(),
                findings:   vec!["deny.toml is missing, and is required here".into()],
                blocked:    true,
            }],
            unreadable: None,
            blocked:    true,
        },
        Vec::new(),
    ));
    assert!(text.contains("tassu"), "{text}");
    assert!(text.contains("deny.toml is missing"), "{text}");
    assert!(text.contains("homma repo config init"), "{text}");
}

#[test]
fn a_dirty_tree_shows_up_without_full_and_a_clean_one_does_not() {
    let dirty = WorktreeLine {
        repo:    "arvo".into(),
        branch:  Some("dev".into()),
        clean:   false,
        changes: 3,
        unread:  None,
    };
    let clean = WorktreeLine {
        repo:    "notko".into(),
        branch:  Some("dev".into()),
        clean:   true,
        changes: 0,
        unread:  None,
    };
    let text = rendered(&with(Vec::new(), empty_configs(), vec![dirty, clean]));
    assert!(text.contains("arvo 3 changes on dev"), "{text}");
    assert!(
        !text.contains("notko"),
        "a clean tree reached the summary: {text}"
    );
}

#[test]
fn a_tree_that_could_not_be_read_is_reported_rather_than_passed_over() {
    // A member the manifest declares and nobody has cloned. Silence would
    // read as a clean tree, which is the one thing it is not.
    let text = rendered(&with(Vec::new(), empty_configs(), vec![WorktreeLine {
        repo:    "kolli".into(),
        branch:  None,
        clean:   true,
        changes: 0,
        unread:  Some("no repository at /ws/kolli".into()),
    }]));
    assert!(text.contains("kolli not read"), "{text}");
}

#[test]
fn json_is_never_summarised() {
    // The property a person reading the terminal cannot notice is broken.
    // The document carries every member of every population whatever the
    // human rendering is doing, because a consumer handed one with the
    // healthy entries dropped has no way to tell.
    let r = with(
        vec![
            agent_state("arvo", AgentState::Configured, true),
            agent_state("kamu", AgentState::Partial, false),
        ],
        ConfigReport {
            repos:      vec![
                RepoConfigState {
                    repo:       "arvo".into(),
                    local_path: "/ws/arvo".into(),
                    findings:   Vec::new(),
                    blocked:    false,
                },
                RepoConfigState {
                    repo:       "tassu".into(),
                    local_path: "/ws/tassu".into(),
                    findings:   vec!["deny.toml is missing, and is required here".into()],
                    blocked:    true,
                },
            ],
            unreadable: None,
            blocked:    true,
        },
        vec![
            WorktreeLine {
                repo:    "arvo".into(),
                branch:  Some("dev".into()),
                clean:   true,
                changes: 0,
                unread:  None,
            },
            WorktreeLine {
                repo:    "notko".into(),
                branch:  Some("dev".into()),
                clean:   false,
                changes: 2,
                unread:  None,
            },
        ],
    );
    // The human default drops the healthy ones, which is the contrast.
    let human = rendered(&r);
    assert!(!human.contains("arvo ok"), "{human}");

    let doc = serde_json::to_value(&r).expect("the report serialises");
    assert_eq!(doc["agent"].as_array().unwrap().len(), 2, "{doc}");
    assert_eq!(
        doc["configs"]["repos"].as_array().unwrap().len(),
        2,
        "{doc}"
    );
    assert_eq!(doc["worktrees"].as_array().unwrap().len(), 2, "{doc}");
    // And the healthy ones are the members that must still be there.
    assert_eq!(doc["configs"]["repos"][0]["repo"], "arvo");
    assert_eq!(doc["worktrees"][0]["repo"], "arvo");
}

#[test]
fn the_document_keeps_the_field_names_it_already_had() {
    // Additive is the whole contract with anything reading this. A rename
    // here is a break nobody would see until somebody's script stopped
    // finding a key.
    let doc = serde_json::to_value(report(Vec::new())).expect("the report serialises");
    for key in ["workspace", "forges", "repos", "injected"] {
        assert!(doc.get(key).is_some(), "`{key}` left the document: {doc}");
    }
    for key in ["agent", "configs", "worktrees"] {
        assert!(doc.get(key).is_some(), "`{key}` never arrived: {doc}");
    }
}

#[test]
fn an_unreadable_configs_directory_is_said_once_rather_than_per_repo() {
    let text = rendered(&with(
        Vec::new(),
        ConfigReport {
            repos:      Vec::new(),
            unreadable: Some("no shared configs at /ws/.shared/configs".into()),
            blocked:    false,
        },
        Vec::new(),
    ));
    assert_eq!(
        text.matches("could not be read").count(),
        1,
        "the workspace-level fault was repeated: {text}"
    );
}
