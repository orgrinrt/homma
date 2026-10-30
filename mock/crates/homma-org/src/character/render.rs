//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Finding the identities in a workspace and writing their definitions.
//!
//! Split from the character itself because it is the half that touches the disk:
//! it lists what is authored, checks the whole set, and writes. A child of
//! [`super`], so it reads a [`Character`]'s private fields without widening them.

use std::path::Path;
use std::{fs, io};

use homma_api::ContainedPath;

use super::{Character, CharacterError, escaped, identity_of, is_authored, is_handle};
use crate::workspace::{Layout, link_memory};

/// What a render wrote.
#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub name:        String,
    pub definition:  ContainedPath,
    pub memory_link: ContainedPath,
}

/// Every name under the hands directory whose `character.md` opens with a
/// frontmatter block, sorted. An older persona without one is left out, and so
/// is a directory with no `character.md`; anything else that goes wrong is
/// reported, because skipping it would render fewer identities and say nothing.
pub fn identities(layout: &Layout<'_>) -> Result<Vec<String>, CharacterError> {
    let hands = layout.hands().map_err(escaped)?;
    let entries = match fs::read_dir(hands.as_path()) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(cause) => {
            return Err(CharacterError::Unreadable {
                path: hands.as_path().to_path_buf(),
                cause,
            });
        },
    };
    let unreadable = |path: &Path| {
        let path = path.to_path_buf();
        move |cause| {
            CharacterError::Unreadable {
                path: path.clone(),
                cause,
            }
        }
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unreadable(hands.as_path()))?;
        if !entry.path().is_dir() {
            continue;
        }
        let file = entry.path().join("character.md");
        let text = match fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(cause) => return Err(unreadable(&file)(cause)),
        };
        if !is_authored(&text) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_handle(&name) {
            return Err(CharacterError::BadName(name));
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// The names of every directory under the hands place whose `character.md`
/// opens with a frontmatter block, whatever else is wrong with it or with its
/// neighbours, sorted.
///
/// For a command about something else (the personas) that must keep away from
/// an identity's files without being stopped by an identity it has no business
/// reading closely; [`identities`] is the one that reports what is broken.
pub fn claimed_names(layout: &Layout<'_>) -> Vec<String> {
    let Ok(hands) = layout.hands() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(hands.as_path()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            fs::read_to_string(e.path().join("character.md")).is_ok_and(|t| is_authored(&t))
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Refuse two identities that keep their memory in the same first directory,
/// among the ones about to be written and every other identity there is.
///
/// A sibling that cannot be read is left to say so when it is rendered itself;
/// it cannot be written and so cannot be sharing anything yet.
fn check_owners(layout: &Layout<'_>, batch: &[Character]) -> Result<(), CharacterError> {
    let mut owners: Vec<(String, String)> = batch
        .iter()
        .map(|c| (c.first_memory().as_str().to_string(), c.name.clone()))
        .collect();
    for name in identities(layout)? {
        if batch.iter().any(|c| c.name == name) {
            continue;
        }
        if let Ok(c) = Character::load(layout, &name) {
            owners.push((c.first_memory().as_str().to_string(), name));
        }
    }
    owners.sort();
    for (i, (outer, outer_owner)) in owners.iter().enumerate() {
        for (inner, inner_owner) in &owners[i + 1 ..] {
            if outer == inner {
                return Err(CharacterError::MemoryShared {
                    memory: outer.clone(),
                    first:  outer_owner.clone(),
                    second: inner_owner.clone(),
                });
            }
            // Sorted, so a directory comes before everything inside it; the
            // comparison is by component, so `a/b` does not contain `a/bc`.
            if Path::new(inner).starts_with(outer) {
                return Err(CharacterError::MemoryNested {
                    outer:       outer.clone(),
                    inner:       inner.clone(),
                    outer_owner: outer_owner.clone(),
                    inner_owner: inner_owner.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Refuse, before anything is written, a place the render would write to that
/// holds something it cannot replace: a real directory or file where the
/// memory link goes, a directory where the definition goes, or something that
/// is not a directory where the memory is kept.
fn check_slots(layout: &Layout<'_>, batch: &[Character]) -> Result<(), CharacterError> {
    for c in batch {
        let id = identity_of(&c.name);
        let link = layout.harness_memory(&id).map_err(escaped)?;
        let definition = layout.definition(&id).map_err(escaped)?;
        for (path, what, fits) in [
            (
                &link,
                "is not a link, and the harness's memory for this identity would be written \
                 there instead of in the shared place",
                (|m: &fs::Metadata| m.file_type().is_symlink()) as fn(&fs::Metadata) -> bool,
            ),
            (
                &definition,
                "is a directory where the definition goes",
                |m: &fs::Metadata| !m.is_dir(),
            ),
        ] {
            match fs::symlink_metadata(path.as_path()) {
                Ok(m) if !fits(&m) => {
                    return Err(CharacterError::Occupied {
                        path: path.as_path().to_path_buf(),
                        what,
                    });
                },
                Ok(_) => {},
                Err(e) if e.kind() == io::ErrorKind::NotFound => {},
                Err(cause) => {
                    return Err(CharacterError::Write {
                        path: path.as_path().to_path_buf(),
                        cause,
                    });
                },
            }
        }
        // The directory the link points at, followed: a file there fails the
        // write after the identities before it were written.
        let memory = layout
            .contained_root()
            .contain(
                &layout
                    .contained_root()
                    .as_abs()
                    .join(c.first_memory().as_str()),
            )
            .map_err(escaped)?;
        match fs::metadata(memory.as_path()) {
            Ok(m) if !m.is_dir() => {
                return Err(CharacterError::Occupied {
                    path: memory.as_path().to_path_buf(),
                    what: "is not a directory, and it is where this identity keeps its memory",
                });
            },
            Ok(_) => {},
            Err(e) if e.kind() == io::ErrorKind::NotFound => {},
            Err(cause) => {
                return Err(CharacterError::Write {
                    path: memory.as_path().to_path_buf(),
                    cause,
                });
            },
        }
    }
    Ok(())
}

/// Write the definition of each of `names`, or of every identity when none is
/// named, and link each one's memory where the harness reads it.
///
/// **All of them are read and checked before any is written**, so one that
/// cannot be rendered leaves the others as they were.
pub fn render(
    layout: &Layout<'_>,
    names: &[String],
    discipline: &str,
) -> Result<Vec<Rendered>, CharacterError> {
    let names = if names.is_empty() { identities(layout)? } else { names.to_vec() };
    let batch = names
        .iter()
        .map(|n| Character::load(layout, n))
        .collect::<Result<Vec<_>, _>>()?;
    check_owners(layout, &batch)?;
    check_slots(layout, &batch)?;
    batch
        .iter()
        .map(|c| write_one(layout, c, discipline))
        .collect()
}

fn write_one(
    layout: &Layout<'_>,
    character: &Character,
    discipline: &str,
) -> Result<Rendered, CharacterError> {
    let root = layout.contained_root();
    let id = identity_of(&character.name);
    let write = |path: &ContainedPath| {
        let path = path.as_path().to_path_buf();
        move |cause| {
            CharacterError::Write {
                path: path.clone(),
                cause,
            }
        }
    };
    // A parent derived from a proven path is proven again before it is created.
    let parent_of = |p: &ContainedPath| -> Result<ContainedPath, CharacterError> {
        let parent = p.as_abs().parent().ok_or_else(|| {
            CharacterError::Escaped(format!("{} has no parent", p.as_path().display()))
        })?;
        root.contain(&parent).map_err(escaped)
    };

    let definition = layout.definition(&id).map_err(escaped)?;
    let target = root
        .contain(&root.as_abs().join(character.first_memory().as_str()))
        .map_err(escaped)?;
    root.create_dir_all(&target).map_err(write(&target))?;
    let link = layout.harness_memory(&id).map_err(escaped)?;
    let dir = parent_of(&link)?;
    root.create_dir_all(&dir).map_err(write(&dir))?;
    link_memory(&link, &target).map_err(write(&link))?;

    // The definition goes last: a session cannot launch as an identity whose
    // memory is not yet where the harness looks, so a failure above leaves an
    // identity that does not start rather than one that starts without memory.
    let dir = parent_of(&definition)?;
    root.create_dir_all(&dir).map_err(write(&dir))?;
    root.write(&definition, character.definition(discipline))
        .map_err(write(&definition))?;

    Ok(Rendered {
        name: character.name.clone(),
        definition,
        memory_link: link,
    })
}
