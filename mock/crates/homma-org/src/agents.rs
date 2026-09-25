//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The agent corpus: the personas a session can dispatch.
//!
//! A persona is one file, authored as `.shared/agents/<name>.md.tmpl` and
//! generated into `.claude/agents/<name>.md`, where it is never edited by hand.
//! Same split as the rules and the skills.
//!
//! # What is read and what is rendered
//!
//! **The whole file is a template and is rendered whole.** Nothing in its
//! frontmatter is re-serialised, so a field the host reads and this does not
//! interpret, nested or not, arrives exactly as it was written. Only two
//! top-level keys are read, `name` and `description`: the first because it has
//! to equal the filename, since the host dispatches by the declared name and a
//! reader finds the file by the other, and the second because the listing says
//! it. A file in the corpus without the `.md.tmpl` suffix is not a persona and
//! is left alone, which is where the corpus's own readme sits.
//!
//! A generated persona ends in exactly one newline, whatever the template
//! ended on, so a template's trailing blank lines never show as a change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use mockspace_template::template::TemplateEnv;
use serde::Serialize;

/// Suffix of an authored persona.
const SUFFIX: &str = ".md.tmpl";
/// Suffix of a generated one.
const GENERATED_SUFFIX: &str = ".md";

/// One persona, as authored.
#[derive(Debug, Clone)]
pub struct Persona {
    /// Its name, which is its filename without the suffix.
    pub name:        String,
    /// When to reach for it.
    pub description: String,
    /// Where it is authored.
    pub path:        PathBuf,
    /// The template, as read.
    source:          String,
}

/// Everything under one `.shared/agents/` directory.
#[derive(Debug, Clone, Default)]
pub struct Personas {
    /// Every persona read, sorted by name.
    pub personas: Vec<Persona>,
}

/// Why a persona corpus could not be read or rendered.
#[derive(Debug)]
pub enum AgentsError {
    /// The directory or a file in it could not be read.
    Unreadable {
        path:  PathBuf,
        cause: io::Error,
    },
    /// The file opens with no frontmatter block, or never closes one.
    NoFrontmatter {
        path: PathBuf,
    },
    /// The frontmatter names no `name` or no `description` at its top level.
    Missing {
        path: PathBuf,
        key:  &'static str,
    },
    /// `name` or `description` is written in a shape this does not read: a
    /// folded or literal block, or a value with a comment after it.
    BadValue {
        path:  PathBuf,
        key:   &'static str,
        value: String,
    },
    /// The declared name and the filename disagree.
    NameMismatch {
        path:     PathBuf,
        declared: String,
        file:     String,
    },
    /// The template does not render.
    Render {
        path:   PathBuf,
        reason: String,
    },
    /// Writing a generated persona failed.
    Write {
        path:  PathBuf,
        cause: io::Error,
    },
}

impl std::fmt::Display for AgentsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::NoFrontmatter {
                path,
            } => {
                write!(
                    f,
                    "{}: no frontmatter block between two `---` lines, so it declares no name",
                    path.display()
                )
            },
            Self::Missing {
                path,
                key,
            } => {
                write!(
                    f,
                    "{}: the frontmatter has no `{key}` at its top level, or it is empty",
                    path.display()
                )
            },
            Self::BadValue {
                path,
                key,
                value,
            } => {
                write!(
                    f,
                    "{}: `{key}: {value}` is a folded or literal block, or carries a comment, \
                     and only a plain or quoted value on its own line is read; write it on one \
                     line, quoted where it holds a ` #`",
                    path.display()
                )
            },
            Self::NameMismatch {
                path,
                declared,
                file,
            } => {
                write!(
                    f,
                    "{}: declares the name `{declared}` and is the file `{file}`, so it is \
                     dispatched under one name and found under another",
                    path.display()
                )
            },
            Self::Render {
                path,
                reason,
            } => write!(f, "{}: {reason}", path.display()),
            Self::Write {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
        }
    }
}

impl std::error::Error for AgentsError {}

/// What a persona's template renders against.
#[derive(Debug, Serialize)]
struct Ctx<'a> {
    name: &'a str,
}

impl Personas {
    /// Read every persona under `dir`.
    pub fn load(dir: &Path) -> Result<Self, AgentsError> {
        let unreadable = |cause| {
            AgentsError::Unreadable {
                path: dir.to_path_buf(),
                cause,
            }
        };
        // Sorted by name, so the order does not depend on the filesystem's.
        let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in fs::read_dir(dir).map_err(unreadable)? {
            let entry = entry.map_err(unreadable)?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| f.strip_suffix(SUFFIX))
            else {
                continue;
            };
            files.insert(name.to_string(), path);
        }

        let mut personas = Vec::with_capacity(files.len());
        for (file, path) in files {
            let source = fs::read_to_string(&path).map_err(|cause| {
                AgentsError::Unreadable {
                    path: path.clone(),
                    cause,
                }
            })?;
            let (declared, description) = declared(&source, &path)?;
            if declared != file {
                return Err(AgentsError::NameMismatch {
                    path,
                    declared,
                    file,
                });
            }
            personas.push(Persona {
                name: file,
                description,
                path,
                source,
            });
        }
        Ok(Self {
            personas,
        })
    }

    /// Render every persona into `dst`, returning the files written.
    ///
    /// Every template is rendered before anything is written, so one that
    /// does not render leaves the generated directory as it was rather than
    /// half rewritten.
    pub fn render(&self, dst: &Path) -> Result<Vec<PathBuf>, AgentsError> {
        let mut rendered = Vec::with_capacity(self.personas.len());
        for p in &self.personas {
            // A fresh environment per render, so a template cannot resolve a
            // name another persona happened to define.
            let out = TemplateEnv::new()
                .render_str(&p.source, &Ctx {
                    name: &p.name,
                })
                .map_err(|e| {
                    AgentsError::Render {
                        path:   p.path.clone(),
                        reason: e.to_string(),
                    }
                })?;
            rendered.push((dst.join(format!("{}{GENERATED_SUFFIX}", p.name)), out));
        }

        fs::create_dir_all(dst).map_err(|cause| {
            AgentsError::Write {
                path: dst.to_path_buf(),
                cause,
            }
        })?;
        let mut written = Vec::with_capacity(rendered.len());
        for (target, out) in rendered {
            // One trailing newline, whatever the template ended on.
            fs::write(&target, format!("{}\n", out.trim_end())).map_err(|cause| {
                AgentsError::Write {
                    path: target.clone(),
                    cause,
                }
            })?;
            written.push(target);
        }
        Ok(written)
    }

    /// Generated personas in `dst` that no authored one claims.
    ///
    /// Reported and never removed: on disk one is the same as a file somebody
    /// put there, and a persona left in the directory is offered to every
    /// session until somebody says so.
    pub fn unclaimed(&self, dst: &Path) -> Result<Vec<String>, AgentsError> {
        if !dst.is_dir() {
            return Ok(Vec::new());
        }
        let unreadable = |cause| {
            AgentsError::Unreadable {
                path: dst.to_path_buf(),
                cause,
            }
        };
        let mut stray = Vec::new();
        for entry in fs::read_dir(dst).map_err(unreadable)? {
            let path = entry.map_err(unreadable)?.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|f| f.strip_suffix(GENERATED_SUFFIX))
            else {
                continue;
            };
            if !self.personas.iter().any(|p| p.name == name) {
                stray.push(name.to_string());
            }
        }
        stray.sort();
        Ok(stray)
    }
}

/// The top-level `name` and `description` a persona's frontmatter declares.
///
/// Read off the lines rather than parsed as a whole, since the host's own
/// fields may nest and none of them is this corpus's to interpret. A line is
/// top level when it does not start with whitespace. Either key written as a
/// folded or literal block, or with a comment after its value, is refused by
/// [`plain`] rather than read as text the author did not mean.
fn declared(source: &str, path: &Path) -> Result<(String, String), AgentsError> {
    let no_block = || {
        AgentsError::NoFrontmatter {
            path: path.to_path_buf(),
        }
    };
    let mut lines = source.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(no_block());
    }
    let mut name = None;
    let mut description = None;
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "name" => name = Some(plain(value, "name", path)?),
            "description" => description = Some(plain(value, "description", path)?),
            _ => {},
        }
    }
    if !closed {
        return Err(no_block());
    }
    let present = |v: Option<String>, key| {
        v.filter(|v| !v.is_empty()).ok_or(AgentsError::Missing {
            path: path.to_path_buf(),
            key,
        })
    };
    Ok((present(name, "name")?, present(description, "description")?))
}

/// One key's value as written after the colon, with its quotes taken off.
///
/// A value opening with `>` or `|` is a folded or literal block whose text is
/// on the lines below, and a `#` after a space or a tab outside a quoted value
/// starts a comment, which YAML drops and a line read would keep. Both are
/// refused. A value quoted whole may carry one, since there it is text.
fn plain(raw: &str, key: &'static str, path: &Path) -> Result<String, AgentsError> {
    let v = raw.trim();
    // Quoted whole only where the quote closes at the end and nowhere before
    // it, its escaped form aside: `\"` inside double quotes, `''` inside single.
    let quoted_whole = [('"', "\\\""), ('\'', "''")]
        .into_iter()
        .any(|(q, escaped)| {
            v.len() >= 2
                && v.starts_with(q)
                && v.ends_with(q)
                && !v[1 .. v.len() - 1].replace(escaped, "").contains(q)
        });
    let block = v.starts_with('>') || v.starts_with('|');
    let after_space = v
        .char_indices()
        .any(|(i, c)| c == '#' && v[.. i].ends_with(char::is_whitespace));
    let comment = !quoted_whole && (v.starts_with('#') || after_space);
    if block || comment {
        return Err(AgentsError::BadValue {
            path: path.to_path_buf(),
            key,
            value: v.to_string(),
        });
    }
    Ok(homma_api::frontmatter::unquote(v))
}
