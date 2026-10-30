//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the character tests share: a workspace root, an authored character, and
//! a render over a layout of it.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use homma_api::{AbsPath, Denied, Paths};
use homma_org::Layout;
use homma_org::character::{CharacterError, Rendered, render};

pub const DISCIPLINE: &str = "## Standing\n\nRead the rules first.";

pub const COORD: &str = "---\nname: coord\nrole: the kaski coordinator, who dispatches helpers\nmemories: [.shared/hands/coord/memory]\n---\n\n# coord\n\n## Who they are\n\n- The persistent coordinator.\n\n## Personality\n\nPlain and short.\n";

/// A workspace root unique per call, since the tests in one binary share a process id.
pub fn root() -> PathBuf {
    static NTH: AtomicUsize = AtomicUsize::new(0);
    let nth = NTH.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("homma-characters-{}-{nth}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".shared/hands")).unwrap();
    fs::canonicalize(dir).unwrap()
}

pub fn author(root: &Path, name: &str, text: &str) {
    let dir = root.join(".shared/hands").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("character.md"), text).unwrap();
}

pub fn layout<'a>(root: &Path, paths: &'a Paths) -> Layout<'a> {
    Layout::new(
        &AbsPath::new(root).unwrap(),
        paths,
        Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
    )
    .unwrap()
}

pub fn go_with(
    root: &Path,
    paths: &Paths,
    names: &[&str],
) -> Result<Vec<Rendered>, CharacterError> {
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    render(&layout(root, paths), &names, DISCIPLINE)
}

pub fn go(root: &Path, names: &[&str]) -> Result<Vec<Rendered>, CharacterError> {
    go_with(root, &Paths::default(), names)
}

pub fn one(root: &Path, name: &str) -> Rendered {
    go(root, &[name]).unwrap().remove(0)
}

pub fn generated(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(".claude/agents").join(format!("{name}.md"))).unwrap()
}

pub fn with_memories(list: &str) -> String {
    COORD.replace("[.shared/hands/coord/memory]", list)
}
