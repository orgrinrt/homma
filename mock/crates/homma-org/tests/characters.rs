//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A character file becoming the agent definition a session loads, and its
//! memories mapped into `.shared` without a copy.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_org::character::{Character, CharacterError, identities, render};

const DISCIPLINE: &str = "## Standing\n\nRead the rules first.";
const AGENTS: &str = ".claude/agents";
const AGENT_MEMORY: &str = ".claude/agent-memory";

const COORD: &str = "---\nname: coord\nrole: the kaski coordinator, who dispatches helpers\nmemories: [.shared/hands/coord/memory]\n---\n\n# coord\n\n## Who they are\n\n- The persistent coordinator.\n\n## Personality\n\nPlain and short.\n";

/// A workspace root unique per call, since the tests in one binary share a process id.
fn root() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-characters-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".shared/hands")).unwrap();
    dir
}

fn author(root: &Path, name: &str, text: &str) {
    let dir = root.join(".shared/hands").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("character.md"), text).unwrap();
}

fn go(root: &Path, name: &str) -> Result<homma_org::character::Rendered, CharacterError> {
    render(
        root,
        name,
        DISCIPLINE,
        Path::new(AGENTS),
        Path::new(AGENT_MEMORY),
    )
}

fn generated(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(AGENTS).join(format!("{name}.md"))).unwrap()
}

#[test]
fn the_definition_is_named_and_described_from_the_frontmatter() {
    let r = root();
    author(&r, "coord", COORD);
    go(&r, "coord").unwrap();
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
fn the_body_arrives_verbatim() {
    let r = root();
    author(&r, "coord", COORD);
    go(&r, "coord").unwrap();
    let out = generated(&r, "coord");
    let body = COORD.split("---\n\n").nth(1).unwrap().trim_end();
    assert!(out.contains(body), "{out}");
}

#[test]
fn an_edit_of_the_personality_reaches_the_generated_file() {
    let r = root();
    author(&r, "coord", COORD);
    go(&r, "coord").unwrap();
    assert!(generated(&r, "coord").contains("Plain and short."));

    let edited = COORD.replace("Plain and short.", "Slow and thorough, and kind about it.");
    assert_ne!(edited, COORD, "the edit must change the file");
    author(&r, "coord", &edited);
    go(&r, "coord").unwrap();
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
    let done = go(&r, "coord").unwrap();
    assert_eq!(done.memory_link, r.join(".claude/agent-memory/coord"));
    assert!(
        fs::symlink_metadata(&done.memory_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::canonicalize(&done.memory_link).unwrap(),
        fs::canonicalize(r.join(".shared/hands/coord/memory")).unwrap()
    );
    assert!(
        fs::read_link(&done.memory_link).unwrap().is_relative(),
        "a committed link must resolve in another clone"
    );
}

#[test]
fn rendering_twice_replaces_the_link_and_changes_nothing() {
    let r = root();
    author(&r, "coord", COORD);
    go(&r, "coord").unwrap();
    let first = generated(&r, "coord");
    go(&r, "coord").unwrap();
    assert_eq!(generated(&r, "coord"), first);
}

#[test]
fn without_a_memories_key_the_memory_is_the_one_beside_the_character() {
    let r = root();
    author(
        &r,
        "coord",
        &COORD.replace("memories: [.shared/hands/coord/memory]\n", ""),
    );
    let c = Character::load(&r, "coord").unwrap();
    assert_eq!(c.memories, vec![".shared/hands/coord/memory".to_string()]);
}

#[test]
fn a_further_memory_is_named_to_be_read_in_place_and_not_linked() {
    let r = root();
    author(
        &r,
        "coord",
        &COORD.replace(
            "[.shared/hands/coord/memory]",
            "[.shared/hands/coord/memory, .shared/projects/kaski/memory]",
        ),
    );
    let done = go(&r, "coord").unwrap();
    let out = generated(&r, "coord");
    assert!(
        out.contains("`.shared/projects/kaski/memory` (read in place)"),
        "{out}"
    );
    assert_eq!(
        fs::canonicalize(done.memory_link).unwrap(),
        fs::canonicalize(r.join(".shared/hands/coord/memory")).unwrap()
    );
}

#[test]
fn a_memory_outside_shared_or_leaving_it_is_refused() {
    for bad in ["/etc", "../outside", ".shared/../outside", "elsewhere/memory"] {
        let r = root();
        author(
            &r,
            "coord",
            &COORD.replace("[.shared/hands/coord/memory]", &format!("[{bad}]")),
        );
        let err = go(&r, "coord").expect_err(bad);
        assert!(
            matches!(err, CharacterError::BadMemory { .. }),
            "{bad}: {err}"
        );
        assert!(
            !r.join(AGENTS).join("coord.md").exists(),
            "{bad}: nothing is written"
        );
    }
}

#[test]
fn a_declared_name_that_is_not_the_directory_is_refused() {
    let r = root();
    author(&r, "coord", &COORD.replace("name: coord", "name: other"));
    assert!(matches!(
        go(&r, "coord"),
        Err(CharacterError::NameMismatch { .. })
    ));
}

#[test]
fn a_name_that_is_not_a_plain_handle_is_refused() {
    let r = root();
    for bad in ["", "..", "a/b", "a b"] {
        assert!(
            matches!(go(&r, bad), Err(CharacterError::BadName(_))),
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
        go(&r, "coord"),
        Err(CharacterError::Frontmatter { .. })
    ));
}

#[test]
fn a_real_directory_where_the_link_goes_is_not_replaced() {
    let r = root();
    author(&r, "coord", COORD);
    fs::create_dir_all(r.join(".claude/agent-memory/coord")).unwrap();
    fs::write(r.join(".claude/agent-memory/coord/note.md"), "kept").unwrap();
    assert!(matches!(
        go(&r, "coord"),
        Err(CharacterError::LinkOccupied(_))
    ));
    assert_eq!(
        fs::read_to_string(r.join(".claude/agent-memory/coord/note.md")).unwrap(),
        "kept"
    );
}

#[test]
fn an_older_persona_without_frontmatter_is_not_an_identity() {
    let r = root();
    author(&r, "coord", COORD);
    author(&r, "aku", "# Aku\n\n## Who he is\n\n- Holds arvo.\n");
    assert_eq!(identities(&r).unwrap(), vec!["coord".to_string()]);
}
