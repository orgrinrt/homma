//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A character file becoming the agent definition a session loads, and its
//! memories mapped into `.shared` without a copy.

mod character_support;

use std::fs;
use std::path::Path;
use std::time::Duration;

use character_support::*;
use homma_api::Paths;
use homma_org::character::{Character, CharacterError, MemoryDir};

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
