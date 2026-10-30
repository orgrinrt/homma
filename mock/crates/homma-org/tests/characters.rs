//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A character file becoming the agent definition a session loads, and its
//! memories mapped into `.shared` without a copy.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{fs, io};

use homma_api::path::RelPath;
use homma_api::{AbsPath, Denied, Paths};
use homma_org::Layout;
use homma_org::character::{Character, CharacterError, MemoryDir, Rendered, identities, render};

const DISCIPLINE: &str = "## Standing\n\nRead the rules first.";

const COORD: &str = "---\nname: coord\nrole: the kaski coordinator, who dispatches helpers\nmemories: [.shared/hands/coord/memory]\n---\n\n# coord\n\n## Who they are\n\n- The persistent coordinator.\n\n## Personality\n\nPlain and short.\n";

/// A workspace root unique per call, since the tests in one binary share a process id.
fn root() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-characters-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".shared/hands")).unwrap();
    fs::canonicalize(dir).unwrap()
}

fn author(root: &Path, name: &str, text: &str) {
    let dir = root.join(".shared/hands").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("character.md"), text).unwrap();
}

fn layout<'a>(root: &Path, paths: &'a Paths) -> Layout<'a> {
    Layout::new(
        &AbsPath::new(root).unwrap(),
        paths,
        Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
    )
    .unwrap()
}

fn go_with(root: &Path, paths: &Paths, names: &[&str]) -> Result<Vec<Rendered>, CharacterError> {
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    render(&layout(root, paths), &names, DISCIPLINE)
}

fn go(root: &Path, names: &[&str]) -> Result<Vec<Rendered>, CharacterError> {
    go_with(root, &Paths::default(), names)
}

fn one(root: &Path, name: &str) -> Rendered {
    go(root, &[name]).unwrap().remove(0)
}

fn generated(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(".claude/agents").join(format!("{name}.md"))).unwrap()
}

fn with_memories(list: &str) -> String {
    COORD.replace("[.shared/hands/coord/memory]", list)
}

#[test]
fn the_definition_is_named_and_described_from_the_frontmatter() {
    let r = root();
    author(&r, "coord", COORD);
    one(&r, "coord");
    let out = generated(&r, "coord");
    assert!(out.starts_with("---\nname: coord\n"), "{out}");
    assert!(
        out.contains("description: the kaski coordinator, who dispatches helpers\n"),
        "{out}"
    );
    assert!(out.contains("memory: project\n"), "{out}");
    assert!(out.contains("Read the rules first."), "{out}");
}

#[test]
fn a_role_with_a_colon_or_a_hash_is_written_as_text_not_as_yaml_structure() {
    for (role, written) in [
        (
            "\"the coordinator: lands branches\"",
            "\"the coordinator: lands branches\"",
        ),
        (
            "\"lands branches # and reads\"",
            "\"lands branches # and reads\"",
        ),
        ("\"[bracketed] role\"", "\"[bracketed] role\""),
    ] {
        let r = root();
        author(
            &r,
            "coord",
            &COORD.replace(
                "role: the kaski coordinator, who dispatches helpers",
                &format!("role: {role}"),
            ),
        );
        one(&r, "coord");
        assert!(
            generated(&r, "coord").contains(&format!("description: {written}\n")),
            "{role}"
        );
    }
}

#[test]
fn the_body_arrives_verbatim() {
    let r = root();
    author(&r, "coord", COORD);
    one(&r, "coord");
    let out = generated(&r, "coord");
    let body = COORD.split("---\n\n").nth(1).unwrap().trim_end();
    assert!(out.contains(body), "{out}");
}

#[test]
fn a_body_that_starts_on_the_line_after_the_fence_keeps_its_first_line() {
    let r = root();
    author(
        &r,
        "coord",
        "---\nname: coord\nrole: the coordinator\n---\n# a heading\n\nFirst para.\n",
    );
    one(&r, "coord");
    let out = generated(&r, "coord");
    assert!(out.contains("# a heading\n\nFirst para."), "{out}");
}

#[test]
fn the_generated_definition_has_one_heading_and_no_twin() {
    let r = root();
    author(&r, "coord", COORD);
    one(&r, "coord");
    let out = generated(&r, "coord");
    assert_eq!(out.matches("\n# ").count(), 1, "{out}");
    assert!(!out.contains("twin"), "{out}");
}

#[test]
fn an_edit_of_the_personality_reaches_the_generated_file() {
    let r = root();
    author(&r, "coord", COORD);
    one(&r, "coord");
    assert!(generated(&r, "coord").contains("Plain and short."));

    let edited = COORD.replace("Plain and short.", "Slow and thorough, and kind about it.");
    assert_ne!(edited, COORD, "the edit must change the file");
    author(&r, "coord", &edited);
    one(&r, "coord");
    let out = generated(&r, "coord");
    assert!(
        out.contains("Slow and thorough, and kind about it."),
        "{out}"
    );
    assert!(!out.contains("Plain and short."), "{out}");
}

#[test]
fn the_memory_link_resolves_to_the_memory_under_shared() {
    let r = root();
    author(&r, "coord", COORD);
    let done = one(&r, "coord");
    assert_eq!(
        done.memory_link.as_path(),
        r.join(".claude/agent-memory/coord")
    );
    assert!(
        fs::symlink_metadata(done.memory_link.as_path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::canonicalize(done.memory_link.as_path()).unwrap(),
        r.join(".shared/hands/coord/memory")
    );
    assert_eq!(
        fs::read_link(done.memory_link.as_path()).unwrap(),
        Path::new("../../.shared/hands/coord/memory"),
        "relative, so a clone elsewhere resolves it"
    );
}

#[test]
fn rendering_twice_changes_nothing_and_leaves_a_correct_link_alone() {
    let r = root();
    author(&r, "coord", COORD);
    let link = one(&r, "coord").memory_link;
    let first = generated(&r, "coord");
    let stamp = fs::symlink_metadata(link.as_path())
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(Duration::from_millis(20));

    let again = one(&r, "coord").memory_link;
    assert_eq!(generated(&r, "coord"), first);
    assert_eq!(
        fs::read_link(again.as_path()).unwrap(),
        Path::new("../../.shared/hands/coord/memory")
    );
    assert_eq!(
        fs::symlink_metadata(again.as_path())
            .unwrap()
            .modified()
            .unwrap(),
        stamp,
        "a link that is already right is not removed and made again"
    );
}

#[test]
fn a_link_pointing_somewhere_else_is_replaced() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".claude/agent-memory")).unwrap();
    std::os::unix::fs::symlink(
        "../../.shared/hands/old",
        r.join(".claude/agent-memory/coord"),
    )
    .unwrap();
    let done = one(&r, "coord");
    assert_eq!(
        fs::read_link(done.memory_link.as_path()).unwrap(),
        Path::new("../../.shared/hands/coord/memory")
    );
}

#[test]
fn without_a_memories_key_the_memory_is_the_one_beside_the_character() {
    let r = root();
    author(
        &r,
        "coord",
        &COORD.replace("memories: [.shared/hands/coord/memory]\n", ""),
    );
    let paths = Paths::default();
    let c = Character::load(&layout(&r, &paths), "coord").unwrap();
    assert_eq!(c.memories(), [
        MemoryDir::new(".shared/hands/coord/memory").unwrap()
    ]);
}

#[test]
fn the_first_memory_is_named_as_the_one_written_to() {
    let r = root();
    author(&r, "coord", COORD);
    one(&r, "coord");
    assert!(
        generated(&r, "coord")
            .contains("- `.shared/hands/coord/memory` (yours, read at the start and written to)\n"),
        "{}",
        generated(&r, "coord")
    );
}

#[test]
fn a_further_memory_is_named_to_be_read_in_place_and_not_linked() {
    let r = root();
    author(
        &r,
        "coord",
        &with_memories("[.shared/hands/coord/memory, .shared/projects/kaski/memory]"),
    );
    let done = one(&r, "coord");
    let out = generated(&r, "coord");
    assert!(
        out.contains("`.shared/projects/kaski/memory` (read in place)"),
        "{out}"
    );
    assert_eq!(
        fs::canonicalize(done.memory_link.as_path()).unwrap(),
        r.join(".shared/hands/coord/memory")
    );
}

#[test]
fn a_memory_outside_shared_leaving_it_or_shared_itself_is_refused() {
    for bad in [
        "/etc",
        "../outside",
        ".shared/../outside",
        "elsewhere/memory",
        ".shared",
        ".shared/",
        ".shared/./x",
    ] {
        let r = root();
        author(&r, "coord", &with_memories(&format!("[{bad}]")));
        let err = go(&r, &["coord"]).expect_err(bad);
        assert!(
            matches!(err, CharacterError::BadMemory { .. }),
            "{bad}: {err}"
        );
        assert!(
            !r.join(".claude/agents/coord.md").exists(),
            "{bad}: nothing is written"
        );
    }
}

#[test]
fn a_memory_directory_cannot_be_made_from_outside_the_type() {
    assert!(MemoryDir::new(".shared/hands/coord/memory").is_some());
    assert!(MemoryDir::new(".shared").is_none());
    assert!(MemoryDir::new("/.shared/x").is_none());
}

#[test]
fn two_identities_keeping_one_memory_are_refused_and_neither_is_written() {
    let r = root();
    author(&r, "coord", COORD);
    author(
        &r,
        "second",
        &COORD
            .replace("name: coord", "name: second")
            .replace("# coord", "# second"),
    );
    let err = go(&r, &[]).expect_err("one memory, two owners");
    assert!(matches!(err, CharacterError::MemoryShared { .. }), "{err}");
    assert!(!r.join(".claude/agents/coord.md").exists());
    assert!(!r.join(".claude/agents/second.md").exists());
}

#[test]
fn naming_one_identity_still_refuses_a_memory_another_keeps() {
    let r = root();
    author(&r, "coord", COORD);
    author(
        &r,
        "second",
        &COORD
            .replace("name: coord", "name: second")
            .replace("# coord", "# second"),
    );
    assert!(matches!(
        go(&r, &["second"]),
        Err(CharacterError::MemoryShared { .. })
    ));
}

#[test]
fn one_that_cannot_be_rendered_stops_the_pass_before_any_is_written() {
    let r = root();
    author(&r, "coord", COORD);
    author(
        &r,
        "second",
        &COORD
            .replace("name: coord", "name: second")
            .replace("# coord", "# second")
            .replace("coord/memory", "second/memory")
            .replace("[.shared/hands/second/memory]", "[../outside]"),
    );
    assert!(go(&r, &[]).is_err());
    assert!(
        !r.join(".claude/agents/coord.md").exists(),
        "the good one waits for the bad one"
    );
}

#[test]
fn a_declared_name_that_is_not_the_directory_is_refused() {
    let r = root();
    author(&r, "coord", &COORD.replace("name: coord", "name: other"));
    assert!(matches!(
        go(&r, &["coord"]),
        Err(CharacterError::NameMismatch { .. })
    ));
}

#[test]
fn a_name_that_is_not_a_plain_handle_is_refused() {
    let r = root();
    for bad in ["..", "a/b", "a b"] {
        assert!(
            matches!(go(&r, &[bad]), Err(CharacterError::BadName(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn a_block_form_memories_list_is_refused_by_the_shared_reader() {
    let r = root();
    author(
        &r,
        "coord",
        &COORD.replace(
            "memories: [.shared/hands/coord/memory]",
            "memories:\n  - .shared/hands/coord/memory",
        ),
    );
    assert!(matches!(
        go(&r, &["coord"]),
        Err(CharacterError::Frontmatter { .. })
    ));
}

#[test]
fn a_real_directory_where_the_link_goes_is_not_replaced() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".claude/agent-memory/coord")).unwrap();
    fs::write(r.join(".claude/agent-memory/coord/note.md"), "kept").unwrap();
    match go(&r, &["coord"]) {
        Err(CharacterError::Write {
            cause,
            ..
        }) => {
            assert_eq!(cause.kind(), io::ErrorKind::AlreadyExists)
        },
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs::read_to_string(r.join(".claude/agent-memory/coord/note.md")).unwrap(),
        "kept"
    );
}

#[test]
fn a_regular_file_where_the_link_goes_is_reported_as_what_it_is() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".claude/agent-memory")).unwrap();
    fs::write(r.join(".claude/agent-memory/coord"), "kept").unwrap();
    let err = go(&r, &["coord"]).expect_err("a file is not a link");
    assert!(err.to_string().contains("is not a symlink"), "{err}");
    assert_eq!(
        fs::read_to_string(r.join(".claude/agent-memory/coord")).unwrap(),
        "kept"
    );
}

#[test]
fn a_claude_directory_linked_out_of_the_root_takes_no_write() {
    let r = root();
    author(&r, "coord", COORD);
    let outside = root();
    fs::create_dir_all(outside.join("deep/.claude")).unwrap();
    std::os::unix::fs::symlink(outside.join("deep/.claude"), r.join(".claude")).unwrap();
    assert!(go(&r, &["coord"]).is_err());
    assert!(!outside.join("deep/.claude/agents/coord.md").exists());
    assert!(!outside.join("deep/.claude/agent-memory").exists());
}

#[test]
fn a_shared_directory_linked_out_of_the_root_is_not_read_or_written_through() {
    let r = root();
    let victim = root();
    fs::create_dir_all(victim.join("hands/coord")).unwrap();
    fs::write(victim.join("hands/coord/character.md"), COORD).unwrap();
    fs::remove_dir_all(r.join(".shared")).unwrap();
    std::os::unix::fs::symlink(&victim, r.join(".shared")).unwrap();
    let _ = go(&r, &["coord"]);
    assert!(!victim.join("hands/coord/memory").exists());
    assert!(!r.join(".claude/agents/coord.md").exists());
}

#[test]
fn the_configured_agents_directory_is_where_the_definition_goes() {
    let r = root();
    author(&r, "coord", COORD);
    let paths = Paths {
        agents: RelPath::new("elsewhere/agents").unwrap(),
        ..Paths::default()
    };
    let done = go_with(&r, &paths, &["coord"]).unwrap().remove(0);
    assert_eq!(
        done.definition.as_path(),
        r.join("elsewhere/agents/coord.md")
    );
    assert!(!r.join(".claude/agents/coord.md").exists());
}

#[test]
fn an_older_persona_without_frontmatter_is_not_an_identity() {
    let r = root();
    author(&r, "coord", COORD);
    author(&r, "aku", "# Aku\n\n## Who he is\n\n- Holds arvo.\n");
    let paths = Paths::default();
    assert_eq!(identities(&layout(&r, &paths)).unwrap(), vec![
        "coord".to_string()
    ]);
}

#[test]
fn files_and_directories_without_a_character_are_not_identities_and_not_errors() {
    let r = root();
    author(&r, "coord", COORD);
    fs::write(r.join(".shared/hands/README.md"), "# hands\n").unwrap();
    fs::create_dir_all(r.join(".shared/hands/empty")).unwrap();
    let paths = Paths::default();
    assert_eq!(identities(&layout(&r, &paths)).unwrap(), vec![
        "coord".to_string()
    ]);
}

#[test]
fn a_character_file_that_cannot_be_read_is_reported_not_skipped() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".shared/hands/broken/character.md")).unwrap();
    let paths = Paths::default();
    assert!(matches!(
        identities(&layout(&r, &paths)),
        Err(CharacterError::Unreadable { .. })
    ));
}

#[test]
fn a_directory_with_frontmatter_and_an_unusable_name_is_reported_not_skipped() {
    let r = root();
    author(&r, "coord", COORD);
    author(&r, "not a handle", COORD);
    let paths = Paths::default();
    assert!(matches!(
        identities(&layout(&r, &paths)),
        Err(CharacterError::BadName(_))
    ));
}

#[test]
fn no_hands_directory_is_no_identities() {
    let r = root();
    fs::remove_dir_all(r.join(".shared/hands")).unwrap();
    let paths = Paths::default();
    assert!(identities(&layout(&r, &paths)).unwrap().is_empty());
    assert!(go(&r, &[]).unwrap().is_empty());
}
