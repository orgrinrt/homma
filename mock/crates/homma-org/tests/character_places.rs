//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The places a render writes to and refuses: who owns a memory, what a slot
//! holds, what is listed, and what a link out of the root may not carry.

mod character_support;

use std::fs;

use character_support::*;
use homma_api::Paths;
use homma_api::path::RelPath;
use homma_org::character::{CharacterError, MemoryDir, identities};

#[test]
fn the_memory_directory_constructor_refuses_the_root_and_what_leaves_it() {
    assert!(MemoryDir::new(".shared/hands/coord/memory").is_some());
    assert!(MemoryDir::new(".shared").is_none());
    assert!(MemoryDir::new("/.shared/x").is_none());
    assert!(MemoryDir::new(".shared/../x").is_none());
}

fn named(name: &str, memory: &str) -> String {
    COORD
        .replace("name: coord", &format!("name: {name}"))
        .replace("# coord", &format!("# {name}"))
        .replace(".shared/hands/coord/memory", memory)
}

#[test]
fn a_memory_inside_another_identitys_memory_is_refused_and_neither_is_written() {
    let r = root();
    author(&r, "outer", &named("outer", ".shared/mem"));
    author(&r, "inner", &named("inner", ".shared/mem/inner"));
    let err = go(&r, &[]).expect_err("a write through one lands in the other");
    assert!(matches!(err, CharacterError::MemoryNested { .. }), "{err}");
    assert!(!r.join(".claude/agents/outer.md").exists());
    assert!(!r.join(".claude/agents/inner.md").exists());
}

#[test]
fn memories_that_only_share_a_name_prefix_do_not_contain_one_another() {
    let r = root();
    author(&r, "aa", &named("aa", ".shared/mem"));
    author(&r, "bb", &named("bb", ".shared/memo"));
    assert_eq!(go(&r, &[]).unwrap().len(), 2);
}

#[test]
fn a_slot_that_cannot_be_replaced_is_found_before_the_first_identity_is_written() {
    let r = root();
    author(&r, "aa", &named("aa", ".shared/aa-mem"));
    author(&r, "bb", &named("bb", ".shared/bb-mem"));
    // The harness made a real directory where bb's link goes.
    fs::create_dir_all(r.join(".claude/agent-memory/bb")).unwrap();
    let err = go(&r, &[]).expect_err("a real directory is not a link");
    assert!(matches!(err, CharacterError::Occupied { .. }), "{err}");
    assert!(
        !r.join(".claude/agents/aa.md").exists(),
        "the identity before it was left alone"
    );
    assert!(!r.join(".claude/agent-memory/aa").exists());
    assert!(!r.join(".shared/aa-mem").exists());
}

#[test]
fn a_directory_where_a_definition_goes_is_found_before_anything_is_written() {
    let r = root();
    author(&r, "aa", &named("aa", ".shared/aa-mem"));
    author(&r, "bb", &named("bb", ".shared/bb-mem"));
    fs::create_dir_all(r.join(".claude/agents/bb.md")).unwrap();
    let err = go(&r, &[]).expect_err("a directory is not a definition");
    assert!(matches!(err, CharacterError::Occupied { .. }), "{err}");
    assert!(!r.join(".claude/agents/aa.md").exists());
}

#[test]
fn a_configured_hands_place_does_not_break_the_default_memory() {
    let r = root();
    let dir = r.join("custom/hands/coord");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("character.md"),
        COORD.replace("memories: [.shared/hands/coord/memory]\n", ""),
    )
    .unwrap();
    let paths = Paths {
        hands: RelPath::new("custom/hands").unwrap(),
        ..Paths::default()
    };
    let done = go_with(&r, &paths, &["coord"]).unwrap().remove(0);
    assert_eq!(done.name, "coord");
    assert!(r.join(".shared/hands/coord/memory").is_dir());
    assert!(
        fs::read_to_string(r.join(".claude/agents/coord.md"))
            .unwrap()
            .contains("`.shared/hands/coord/memory`")
    );
}

#[test]
fn a_handle_cannot_start_with_a_dash() {
    let r = root();
    for bad in ["-x", "--agent"] {
        assert!(
            matches!(go(&r, &[bad]), Err(CharacterError::BadName(_))),
            "{bad}"
        );
    }
    assert!(homma_org::character::is_handle("a-b_c"));
    assert!(!homma_org::character::is_handle("-ab"));
}

#[test]
fn an_older_persona_text_is_not_authored_and_a_frontmatter_one_is() {
    use homma_org::character::is_authored;
    assert!(is_authored(COORD));
    assert!(is_authored("---  \nname: x\n"));
    assert!(!is_authored("# Aku\n\n---\n"));
    assert!(!is_authored(""));
}

#[test]
fn two_identities_keeping_one_memory_are_refused_and_neither_is_written() {
    let r = root();
    author(&r, "coord", &named("coord", ".shared/common-mem"));
    author(&r, "second", &named("second", ".shared/common-mem"));
    let err = go(&r, &[]).expect_err("one memory, two owners");
    assert!(matches!(err, CharacterError::MemoryShared { .. }), "{err}");
    assert!(!r.join(".claude/agents/coord.md").exists());
    assert!(!r.join(".claude/agents/second.md").exists());
}

#[test]
fn naming_one_identity_still_refuses_a_memory_another_keeps() {
    let r = root();
    author(&r, "coord", &named("coord", ".shared/common-mem"));
    author(&r, "second", &named("second", ".shared/common-mem"));
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
    let err = go(&r, &[]).expect_err("a memory that leaves `.shared`");
    assert!(
        matches!(&err, CharacterError::BadMemory { memory, .. } if memory == "../outside"),
        "{err}"
    );
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
        Err(CharacterError::Occupied {
            path,
            ..
        }) => {
            assert_eq!(path, r.join(".claude/agent-memory/coord"))
        },
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs::read_to_string(r.join(".claude/agent-memory/coord/note.md")).unwrap(),
        "kept"
    );
    assert!(
        !r.join(".claude/agents/coord.md").exists(),
        "found before anything was written"
    );
}

#[test]
fn a_regular_file_where_the_link_goes_is_reported_as_what_it_is() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".claude/agent-memory")).unwrap();
    fs::write(r.join(".claude/agent-memory/coord"), "kept").unwrap();
    let err = go(&r, &["coord"]).expect_err("a file is not a link");
    assert!(matches!(err, CharacterError::Occupied { .. }), "{err}");
    assert!(err.to_string().contains("is not a link"), "{err}");
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
    let err = go(&r, &["coord"]).expect_err("the harness directory leaves the root");
    assert!(matches!(err, CharacterError::Escaped(_)), "{err}");
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
    let err = go(&r, &["coord"]).expect_err("the shared directory leaves the root");
    assert!(matches!(err, CharacterError::Escaped(_)), "{err}");
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

#[test]
fn a_file_where_the_memory_is_kept_is_found_before_the_first_identity_is_written() {
    let r = root();
    author(&r, "aa", &named("aa", ".shared/aa-mem"));
    author(&r, "bb", &named("bb", ".shared/bb-mem"));
    fs::write(r.join(".shared/bb-mem"), "a file").unwrap();
    let err = go(&r, &[]).expect_err("a file is not a memory directory");
    assert!(
        matches!(&err, CharacterError::Occupied { path, .. } if path.ends_with(".shared/bb-mem")),
        "{err}"
    );
    assert!(!r.join(".claude/agents/aa.md").exists());
    assert!(!r.join(".claude/agent-memory/aa").exists());
    assert!(!r.join(".shared/aa-mem").exists());
}

#[test]
fn a_memory_under_another_names_hands_directory_is_refused() {
    for memory in [
        ".shared/hands/paja/memory",
        ".shared/hands/paja",
        ".shared/hands",
        ".shared/hands/aa-other/memory",
    ] {
        let r = root();
        author(&r, "aa", &named("aa", memory));
        let err = go(&r, &["aa"]).expect_err(memory);
        assert!(
            matches!(&err, CharacterError::MemoryUnderAnother { name, .. } if name == "aa"),
            "{memory}: {err}"
        );
        assert!(!r.join(".claude/agents/aa.md").exists(), "{memory}");
    }
}

#[test]
fn a_memory_under_its_own_name_below_the_hands_directory_is_kept() {
    let r = root();
    author(&r, "aa", &named("aa", ".shared/hands/aa/deeper/memory"));
    assert_eq!(go(&r, &["aa"]).unwrap().len(), 1);
    assert!(r.join(".shared/hands/aa/deeper/memory").is_dir());
}

#[test]
fn a_name_a_plain_scalar_would_misread_is_written_quoted() {
    let r = root();
    author(&r, "true", &named("true", ".shared/hands/true/memory"));
    author(&r, "123", &named("123", ".shared/hands/123/memory"));
    go(&r, &[]).unwrap();
    assert!(generated(&r, "true").starts_with("---\nname: \"true\"\n"));
    assert!(generated(&r, "123").starts_with("---\nname: \"123\"\n"));
}
