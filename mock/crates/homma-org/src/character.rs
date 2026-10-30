//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A character file, and the identity a session loads from it.
//!
//! An identity is authored as `.shared/hands/<name>/character.md`: a frontmatter
//! block naming it (`name`, `role`, `memories`) and a markdown body whose
//! `## Personality` section is edited by others, so the body is composed into
//! the generated agent definition verbatim and never re-serialised. The
//! definition itself is made by [`crate::generate::definition_described`], the
//! one owner of what an agent definition holds; this module only reads the file
//! and maps its memories.
//!
//! **Memories are mapped, never copied.** The harness reads an agent's memory
//! from `.claude/agent-memory/<name>`, and that is a relative link to the first
//! memory directory the file names, which stays under `.shared`. Any further
//! directory is named in the definition to be read in place. A file without a
//! frontmatter block is an older persona and is not an identity.

use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use homma_api::frontmatter::{self, FrontmatterError};
use homma_api::{Identity, Role};

use crate::generate::{Form, definition_described};

/// Where characters are authored, relative to the workspace root.
pub const HANDS: &str = ".shared/hands";
/// Where the memories of an identity may live, relative to the workspace root.
pub const MEMORY_ROOT: &str = ".shared";
/// The keys the frontmatter may carry.
const KNOWN: &[&str] = &["name", "role", "memories"];

/// A character file, as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// Its name, which is its directory.
    pub name:     String,
    /// What it is for, and the sentence the agent is described by.
    pub role:     String,
    /// Memory directories relative to the workspace root, the first written to.
    pub memories: Vec<String>,
    /// Everything after the frontmatter, as written.
    pub body:     String,
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
    /// A memory path that is absolute, leaves its directory or leaves `.shared`.
    BadMemory {
        path:   PathBuf,
        memory: String,
    },
    /// The harness's memory path holds a real directory the link would replace.
    LinkOccupied(PathBuf),
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
                    "{}: memory `{memory}` must be a relative path under `{MEMORY_ROOT}`, with no \
                     `..`",
                    path.display()
                )
            },
            Self::LinkOccupied(p) => {
                write!(
                    f,
                    "{} is a real directory, not a link, and replacing it would lose what the \
                     harness wrote there",
                    p.display()
                )
            },
        }
    }
}

impl std::error::Error for CharacterError {}

/// Whether `name` is a plain handle, safe as a directory and a file stem.
pub fn is_handle(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Character {
    /// Read `source`, the content of `path`, which sits in the directory `dir`.
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
        let mut memories = block.list("memories").map_err(fm)?;
        if memories.is_empty() {
            memories.push(format!("{HANDS}/{name}/memory"));
        }
        for m in &memories {
            if !under_memory_root(m) {
                return Err(CharacterError::BadMemory {
                    path:   path.to_path_buf(),
                    memory: m.clone(),
                });
            }
        }
        Ok(Self {
            name,
            role,
            memories,
            body: block.body,
        })
    }

    /// Read `<root>/.shared/hands/<name>/character.md`.
    pub fn load(root: &Path, name: &str) -> Result<Self, CharacterError> {
        if !is_handle(name) {
            return Err(CharacterError::BadName(name.to_string()));
        }
        let path = root.join(HANDS).join(name).join("character.md");
        let source = fs::read_to_string(&path).map_err(|cause| {
            CharacterError::Unreadable {
                path: path.clone(),
                cause,
            }
        })?;
        Self::parse(&source, name, &path)
    }

    /// The registry entry the definition is made from.
    pub fn identity(&self) -> Identity {
        Identity::new(Role::Hand, self.name.clone())
    }

    /// The body with the memories named after it, which is what the definition
    /// composes in.
    pub fn compose(&self) -> String {
        let mut out = self.body.trim_end().to_string();
        out.push_str("\n\n## Memories\n\n");
        for (i, m) in self.memories.iter().enumerate() {
            let how =
                if i == 0 { "yours, read at the start and written to" } else { "read in place" };
            out.push_str(&format!("- `{m}` ({how})\n"));
        }
        out
    }

    /// The agent definition a session launched as this identity loads.
    pub fn definition(&self, discipline: &str) -> String {
        definition_described(
            &self.identity(),
            Form::Prime,
            &self.role,
            discipline,
            &self.compose(),
        )
        .render()
    }
}

/// Whether `memory` is a relative path of plain components under `.shared`.
fn under_memory_root(memory: &str) -> bool {
    let p = Path::new(memory);
    p.starts_with(MEMORY_ROOT) && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// What a render wrote.
#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub definition:  PathBuf,
    pub memory_link: PathBuf,
}

/// Every name under `<root>/.shared/hands` whose `character.md` opens with a
/// frontmatter block, sorted. An older persona without one is left out.
pub fn identities(root: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root.join(HANDS))? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(entry.path().join("character.md")) else {
            continue;
        };
        if is_handle(&name) && text.lines().next().map(str::trim_end) == Some("---") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Write the definition of `name` into `agents` and link its memory under
/// `agent_memory`, both relative to `root`.
pub fn render(
    root: &Path,
    name: &str,
    discipline: &str,
    agents: &Path,
    agent_memory: &Path,
) -> Result<Rendered, CharacterError> {
    let character = Character::load(root, name)?;
    let write = |path: &Path| {
        let path = path.to_path_buf();
        move |cause| {
            CharacterError::Write {
                path: path.clone(),
                cause,
            }
        }
    };

    let definition = root.join(agents).join(format!("{name}.md"));
    fs::create_dir_all(root.join(agents)).map_err(write(&root.join(agents)))?;
    fs::write(&definition, character.definition(discipline)).map_err(write(&definition))?;

    let target = root.join(&character.memories[0]);
    fs::create_dir_all(&target).map_err(write(&target))?;
    let link = root.join(agent_memory).join(name);
    fs::create_dir_all(root.join(agent_memory)).map_err(write(&root.join(agent_memory)))?;
    match fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() => fs::remove_file(&link).map_err(write(&link))?,
        Ok(_) => return Err(CharacterError::LinkOccupied(link)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {},
        Err(e) => return Err(write(&link)(e)),
    }
    // Relative, so a clone elsewhere resolves it: one `..` per component of the
    // directory the link sits in, then the memory path.
    let depth = Path::new(agent_memory).components().count();
    let mut rel = PathBuf::new();
    for _ in 0 .. depth {
        rel.push("..");
    }
    std::os::unix::fs::symlink(rel.join(&character.memories[0]), &link).map_err(write(&link))?;
    Ok(Rendered {
        definition,
        memory_link: link,
    })
}
