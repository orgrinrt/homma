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
//! owner, so two identities may not name the same first directory, nor one
//! inside the other's. A file without a frontmatter block is an older persona
//! and is not an identity.

use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use homma_api::frontmatter::{self, FrontmatterError};
use homma_api::{Escapes, Identity, Role};

use crate::generate::{Form, definition_described};
use crate::workspace::Layout;

mod render;

pub use render::{Rendered, claimed_names, identities, render};

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
    /// One identity's first memory directory lies inside another's, so a write
    /// through either lands in the other.
    MemoryNested {
        outer:       String,
        inner:       String,
        outer_owner: String,
        inner_owner: String,
    },
    /// A place a render would write to holds something that is not what it
    /// writes there, found before anything was written.
    Occupied {
        path: PathBuf,
        what: &'static str,
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
                    "`{n}` is not a plain handle: letters, digits, `-` and `_` only, and not \
                     starting with `-`"
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
            Self::MemoryNested {
                outer,
                inner,
                outer_owner,
                inner_owner,
            } => {
                write!(
                    f,
                    "`{inner}` of `{inner_owner}` lies inside `{outer}` of `{outer_owner}`, so a \
                     write to one lands in the other and a memory has one owner"
                )
            },
            Self::Occupied {
                path,
                what,
            } => write!(f, "{}: {what}", path.display()),
            Self::Escaped(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CharacterError {}

fn escaped(e: Escapes) -> CharacterError {
    CharacterError::Escaped(e.to_string())
}

/// Whether `name` is a plain handle, safe as a directory and a file stem, and
/// not readable as an option when it is passed on a command line.
pub fn is_handle(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether the text of a `character.md` is an authored identity: it opens with a
/// frontmatter fence. The one predicate that separates an identity from an older
/// persona, so listing, rendering and standing up cannot disagree about it.
pub fn is_authored(text: &str) -> bool {
    text.lines().next().map(str::trim_end) == Some("---")
}

/// The text of `id`'s `character.md` where the layout places it, empty when
/// there is none. Only absence is absence: any other failure is the caller's.
pub fn read_text(layout: &Layout<'_>, id: &Identity) -> io::Result<String> {
    let path = layout
        .character(id)
        .map_err(|e| io::Error::other(e.to_string()))?;
    match fs::read_to_string(path.as_path()) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

/// Refuse to generate a registry definition for a name whose character is an
/// authored identity's. Its definition is written by `homma identity render`
/// into the same file, so a second writer would be pasting the raw frontmatter
/// block into a body. Asked by `write_definitions` and, before it creates
/// anything, by a stand-up, so the two cannot disagree.
pub fn refuse_authored(text: &str, handle: &str) -> io::Result<()> {
    if is_authored(text) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "`{handle}` has a character file with frontmatter, so it is an identity and its \
                 definition is generated by `homma identity render`"
            ),
        ));
    }
    Ok(())
}

/// Where an identity keeps its memory when its file names none. Fixed, and not
/// derived from the configured hands place: the rule that a memory lives under
/// [`MEMORY_ROOT`] is the character format's, and a layout that puts the
/// characters elsewhere must not make the default fail it.
fn default_memory(name: &str) -> String {
    format!("{MEMORY_ROOT}/hands/{name}/memory")
}

/// The registry entry a definition is made from: a hand by this name.
fn identity_of(name: &str) -> Identity {
    Identity::new(Role::Hand, name.to_string())
}

impl Character {
    /// Read `source`, the content of `path`, which sits in the directory `dir`.
    ///
    /// A file naming no memory keeps it in `.shared/hands/<name>/memory`.
    pub fn parse(source: &str, dir: &str, path: &Path) -> Result<Self, CharacterError> {
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
            raw.push(default_memory(dir));
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
        Self::parse(&source, name, path.as_path())
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
