//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The mode corpus: one file per mode, generated into the skills tree as a skill
//! whose frontmatter is built from the mode's meta.
//!
//! Only `.md.tmpl` files are modes, so a settings file beside them is not one.

use std::path::{Path, PathBuf};
use std::{fs, io};

use homma_api::FrontmatterError;
use homma_api::mode::{self, ModeMeta};
use serde::Serialize;

use crate::surface::{self, Kind, Shape};
use crate::yaml;

/// One mode, as authored.
#[derive(Debug, Clone)]
pub struct Mode {
    /// The mode's type, which is its file's name.
    pub name: String,
    pub meta: ModeMeta,
    /// The body's template source.
    pub body: String,
    /// Where it is authored.
    pub path: PathBuf,
}

/// Every mode under one directory, and the prefix their skills are named with.
#[derive(Debug, Clone, Default)]
pub struct Modes {
    pub modes:  Vec<Mode>,
    pub prefix: String,
}

/// Why a mode corpus could not be read or rendered.
#[derive(Debug)]
pub enum ModesError {
    Unreadable {
        path:  PathBuf,
        cause: io::Error,
    },
    Meta {
        path:  PathBuf,
        cause: FrontmatterError,
    },
    /// The declared type and the file's name disagree.
    TypeMismatch {
        path:     PathBuf,
        declared: String,
    },
    /// The body is over the configured number of characters.
    TooLong {
        path:  PathBuf,
        chars: usize,
        max:   usize,
    },
    Render {
        path:   PathBuf,
        reason: String,
    },
    Write {
        path:  PathBuf,
        cause: io::Error,
    },
}

impl std::fmt::Display for ModesError {
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
            Self::Meta {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::TypeMismatch {
                path,
                declared,
            } => {
                write!(
                    f,
                    "{}: declares the type `{declared}`, and a mode's type is its file's name",
                    path.display()
                )
            },
            Self::TooLong {
                path,
                chars,
                max,
            } => {
                write!(
                    f,
                    "{}: the body is {chars} characters, over the {max} a mode may hold",
                    path.display()
                )
            },
            Self::Render {
                path,
                reason,
            } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for ModesError {}

/// The modes as a kind, bounded by the most characters a body may hold.
struct ModeFiles {
    body_max_chars: usize,
}

impl Kind for ModeFiles {
    type Error = ModesError;
    type Item = Mode;

    const SHAPE: Shape = Shape::File;

    fn unreadable(path: PathBuf, cause: io::Error) -> ModesError {
        ModesError::Unreadable {
            path,
            cause,
        }
    }

    fn read(&self, name: &str, path: &Path) -> Result<Mode, ModesError> {
        let source = surface::read_text::<Self>(path)?;
        let parsed = mode::parse(&source).map_err(|cause| {
            ModesError::Meta {
                path: path.to_path_buf(),
                cause,
            }
        })?;
        if parsed.meta.kind != name {
            return Err(ModesError::TypeMismatch {
                path:     path.to_path_buf(),
                declared: parsed.meta.kind,
            });
        }
        let chars = parsed.body.trim().chars().count();
        if chars > self.body_max_chars {
            return Err(ModesError::TooLong {
                path: path.to_path_buf(),
                chars,
                max: self.body_max_chars,
            });
        }
        Ok(Mode {
            name: name.to_string(),
            meta: parsed.meta,
            body: parsed.body,
            path: path.to_path_buf(),
        })
    }
}

/// What a mode's body renders against.
#[derive(Debug, Serialize)]
struct Ctx<'a> {
    name: &'a str,
    meta: &'a ModeMeta,
}

impl Modes {
    /// Read every mode under `dir`, refusing a type other than the file's name
    /// and a body over `body_max_chars`.
    pub fn load(dir: &Path, prefix: &str, body_max_chars: usize) -> Result<Self, ModesError> {
        Ok(Self {
            modes:  surface::load(
                &ModeFiles {
                    body_max_chars,
                },
                dir,
            )?,
            prefix: prefix.to_string(),
        })
    }

    /// The name of a mode's skill.
    pub fn skill_name(&self, m: &Mode) -> String {
        format!("{}{}", self.prefix, m.name)
    }

    /// Whether `name` is the skill of one of these modes.
    pub fn claims(&self, name: &str) -> bool {
        self.modes.iter().any(|m| self.skill_name(m) == name)
    }

    /// The generated `SKILL.md` of a mode: its frontmatter built from the meta,
    /// then the rendered body.
    pub fn skill_text(&self, m: &Mode) -> Result<String, ModesError> {
        let body = surface::render(&m.body, &Ctx {
            name: &m.name,
            meta: &m.meta,
        })
        .map_err(|reason| {
            ModesError::Render {
                path: m.path.clone(),
                reason,
            }
        })?;
        let description = format!("mode: {}. Use when {}.", m.name, m.meta.when);
        Ok(format!(
            "---\nname: {}\ndescription: {}\neffort: {}\n---\n\n{}",
            yaml::scalar(&self.skill_name(m)),
            yaml::scalar(&description),
            m.meta.effort,
            body.trim()
        ))
    }

    /// Write each mode's skill into `dst`, replacing what an earlier render
    /// wrote there, and return the files written.
    pub fn render(&self, dst: &Path) -> Result<Vec<PathBuf>, ModesError> {
        let mut written = Vec::with_capacity(self.modes.len());
        for m in &self.modes {
            let dir = dst.join(self.skill_name(m));
            self.remove_one(&dir)?;
            fs::create_dir_all(&dir).map_err(|cause| {
                ModesError::Write {
                    path: dir.clone(),
                    cause,
                }
            })?;
            let file = dir.join("SKILL.md");
            surface::write(&file, &self.skill_text(m)?).map_err(|cause| {
                ModesError::Write {
                    path: file.clone(),
                    cause,
                }
            })?;
            written.push(file);
        }
        Ok(written)
    }

    /// Remove every mode skill an earlier render wrote into `dst`: the modes
    /// claim them, so they are this pass's own.
    pub fn remove(&self, dst: &Path) -> Result<Vec<String>, ModesError> {
        let mut removed = Vec::new();
        for m in &self.modes {
            let name = self.skill_name(m);
            if self.remove_one(&dst.join(&name))? {
                removed.push(name);
            }
        }
        Ok(removed)
    }

    fn remove_one(&self, dir: &Path) -> Result<bool, ModesError> {
        match fs::remove_dir_all(dir) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(cause) => {
                Err(ModesError::Write {
                    path: dir.to_path_buf(),
                    cause,
                })
            },
        }
    }
}
