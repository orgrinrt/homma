//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A character file, and the identity a session loads from it.
//!
//! An identity is authored as `<hands>/<name>/character.md`: a frontmatter block
//! naming it (`name`, `role`, `memories`) and a markdown body whose
//! `## Personality` section is edited by others, so the body is composed into
//! the generated agent definition verbatim and never re-serialised. The
//! definition itself is made by [`crate::generate::definition_described`], the
//! one owner of what an agent definition holds; this module only reads the file
//! and maps its memories.
//!
//! **Every path comes from the [`Layout`]**, the one owner of where things sit in
//! a workspace and of proving they stay inside it, so the configured `hands` and
//! `agents` places count and a link in the tree cannot carry a write out of the
//! root. Nothing here builds a path by hand.
//!
//! **Memories are mapped, never copied.** The harness reads an agent's memory
//! from `.claude/agent-memory/<name>`, and that is a relative link to the first
//! memory directory the file names, which stays under `.shared`. Any further
//! directory is named in the definition to be read in place. A memory has one
//! owner, so two identities may not name the same first directory. A file
//! without a frontmatter block is an older persona and is not an identity.

use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use homma_api::frontmatter::{self, FrontmatterError};
use homma_api::{ContainedPath, Escapes, Identity, Role};

use crate::generate::{Form, definition_described};
use crate::workspace::{Layout, link_memory};

/// Where the memories of an identity may live, relative to the workspace root.
pub const MEMORY_ROOT: &str = ".shared";
/// The keys the frontmatter may carry.
const KNOWN: &[&str] = &["name", "role", "memories"];

/// A memory directory: relative, plain components, and a directory under
/// [`MEMORY_ROOT`] rather than the root itself.
///
/// The only constructor checks that, so a value of this type is one the link
/// and the definition can use without asking again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryDir(String);

impl MemoryDir {
    /// `raw` as a memory directory, or `None` when it is absolute, leaves its
    /// directory, or is not strictly below `.shared`.
    pub fn new(raw: &str) -> Option<Self> {
        let p = Path::new(raw);
        // `components` quietly drops a `.` and an empty segment, so the spelling
        // is checked as well: what is stored is what is linked to.
        let ok = p.starts_with(MEMORY_ROOT)
            && p.components().count() >= 2
            && p.components().all(|c| matches!(c, Component::Normal(_)))
            && raw.split('/').all(|s| !s.is_empty() && s != ".");
        ok.then(|| Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A character file, as authored.
///
/// Its fields are private: the checks in [`Character::parse`] are what make a
/// value of this type safe to render, and a struct literal would skip them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    name:     String,
    role:     String,
    memories: Vec<MemoryDir>,
    body:     String,
}

/// Why a character could not be read or rendered.
#[derive(Debug)]
pub enum CharacterError {
    Unreadable {
        path:  PathBuf,
        cause: io::Error,
    },
    Frontmatter {
        path:  PathBuf,
        cause: FrontmatterError,
    },
    /// The declared name and the directory disagree.
    NameMismatch {
        path:     PathBuf,
        declared: String,
        dir:      String,
    },
    /// A name that is not a plain handle.
    BadName(String),
    /// A memory path that is absolute, leaves its directory or is not below
    /// `.shared`.
    BadMemory {
        path:   PathBuf,
        memory: String,
    },
    /// Two identities name the same first memory directory.
    MemoryShared {
        memory: String,
        first:  String,
        second: String,
    },
    /// A path the layout refuses: outside the root, or somewhere denied.
    Escaped(String),
    Write {
        path:  PathBuf,
        cause: io::Error,
    },
}

impl std::fmt::Display for CharacterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable {
                path,
                cause,
            }
            | Self::Write {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::Frontmatter {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::NameMismatch {
                path,
                declared,
                dir,
            } => {
                write!(
                    f,
                    "{}: declares the name `{declared}` and lives in `{dir}`, so it is launched \
                     under one name and found under another",
                    path.display()
                )
            },
            Self::BadName(n) => {
                write!(
                    f,
                    "`{n}` is not a plain handle: letters, digits, `-` and `_` only"
                )
            },
            Self::BadMemory {
                path,
                memory,
            } => {
                write!(
                    f,
                    "{}: memory `{memory}` must be a relative path to a directory below \
                     `{MEMORY_ROOT}`, with no `..`",
                    path.display()
                )
            },
            Self::MemoryShared {
                memory,
                first,
                second,
            } => {
                write!(
                    f,
                    "`{first}` and `{second}` both keep their memory in `{memory}`, and a memory \
                     has one owner"
                )
            },
            Self::Escaped(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CharacterError {}

fn escaped(e: Escapes) -> CharacterError {
    CharacterError::Escaped(e.to_string())
}

/// Whether `name` is a plain handle, safe as a directory and a file stem.
pub fn is_handle(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The registry entry a definition is made from: a hand by this name.
fn identity_of(name: &str) -> Identity {
    Identity::new(Role::Hand, name.to_string())
}

impl Character {
    /// Read `source`, the content of `path`, which sits in the directory `dir`.
    ///
    /// `default_memory` is the directory used when the file names none.
    pub fn parse(
        source: &str,
        dir: &str,
        path: &Path,
        default_memory: &str,
    ) -> Result<Self, CharacterError> {
        let fm = |cause| {
            CharacterError::Frontmatter {
                path: path.to_path_buf(),
                cause,
            }
        };
        let block = frontmatter::split(source, KNOWN).map_err(fm)?;
        let name = block.scalar("name").map_err(fm)?;
        let role = block.scalar("role").map_err(fm)?;
        if name != dir {
            return Err(CharacterError::NameMismatch {
                path:     path.to_path_buf(),
                declared: name,
                dir:      dir.to_string(),
            });
        }
        let mut raw = block.list("memories").map_err(fm)?;
        if raw.is_empty() {
            raw.push(default_memory.to_string());
        }
        let memories = raw
            .iter()
            .map(|m| {
                MemoryDir::new(m).ok_or_else(|| {
                    CharacterError::BadMemory {
                        path:   path.to_path_buf(),
                        memory: m.clone(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name,
            role,
            memories,
            body: block.body,
        })
    }

    /// Read the character of `name` from where the layout places it.
    pub fn load(layout: &Layout<'_>, name: &str) -> Result<Self, CharacterError> {
        if !is_handle(name) {
            return Err(CharacterError::BadName(name.to_string()));
        }
        let id = identity_of(name);
        let path = layout.character(&id).map_err(escaped)?;
        let source = fs::read_to_string(path.as_path()).map_err(|cause| {
            CharacterError::Unreadable {
                path: path.as_path().to_path_buf(),
                cause,
            }
        })?;
        Self::parse(&source, name, path.as_path(), &layout.default_memory(&id))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// What it is for, and the sentence the agent is described by.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// Memory directories relative to the workspace root, the first written to.
    pub fn memories(&self) -> &[MemoryDir] {
        &self.memories
    }

    /// Everything after the frontmatter, as written.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The body with the memories named after it, which is what the definition
    /// composes in.
    pub fn compose(&self) -> String {
        let mut out = self.body.trim_end().to_string();
        out.push_str("\n\n## Memories\n\n");
        for (i, m) in self.memories.iter().enumerate() {
            let how =
                if i == 0 { "yours, read at the start and written to" } else { "read in place" };
            out.push_str(&format!("- `{}` ({how})\n", m.as_str()));
        }
        out
    }

    /// The agent definition a session launched as this identity loads.
    pub fn definition(&self, discipline: &str) -> String {
        definition_described(
            &identity_of(&self.name),
            Form::Authored,
            &self.role,
            discipline,
            &self.compose(),
        )
        .render()
    }

    /// The directory this identity writes its memory to.
    fn first_memory(&self) -> &MemoryDir {
        &self.memories[0]
    }
}

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
        if text.lines().next().map(str::trim_end) != Some("---") {
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

/// Whether `name` is an identity: it has a character file with frontmatter.
pub fn is_identity(layout: &Layout<'_>, name: &str) -> bool {
    identities(layout).is_ok_and(|all| all.iter().any(|n| n == name))
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
    for pair in owners.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(CharacterError::MemoryShared {
                memory: pair[0].0.clone(),
                first:  pair[0].1.clone(),
                second: pair[1].1.clone(),
            });
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
    let dir = parent_of(&definition)?;
    root.create_dir_all(&dir).map_err(write(&dir))?;
    root.write(&definition, character.definition(discipline))
        .map_err(write(&definition))?;

    let target = root
        .contain(&root.as_abs().join(character.first_memory().as_str()))
        .map_err(escaped)?;
    root.create_dir_all(&target).map_err(write(&target))?;
    let link = layout.harness_memory(&id).map_err(escaped)?;
    let dir = parent_of(&link)?;
    root.create_dir_all(&dir).map_err(write(&dir))?;
    link_memory(&link, &target).map_err(write(&link))?;

    Ok(Rendered {
        name: character.name.clone(),
        definition,
        memory_link: link,
    })
}
