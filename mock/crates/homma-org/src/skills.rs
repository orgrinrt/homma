//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The skill corpus: what is authored, what is generated, and which skills bear
//! on a rule.
//!
//! A skill is authored as a directory under `.shared/skills/`, holding
//! `SKILL.md.tmpl` and whatever it needs beside it, and is generated into
//! `.claude/skills/` where a session finds it. Same split as the rules and for
//! the same reason: one authored form, one generated form, and the generated
//! one is never edited by hand.
//!
//! # What is rendered and what is copied
//!
//! **A `.md.tmpl` is rendered and loses that suffix. Everything else is copied
//! byte for byte, with its mode.** A skill's scripts and lockfiles are not
//! prose and templating them would mean a shell script could not contain a
//! brace without escaping it. The executable bit is carried across because a
//! skill whose command-line tool arrives unexecutable fails at the moment it is
//! reached for, which is the worst time to find out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use homma_api::skill::{self, SkillError, SkillMeta};
use mockspace_template::template::TemplateEnv;
use serde::Serialize;

/// Suffix of an authored template, anywhere inside a skill.
const TMPL_SUFFIX: &str = ".tmpl";
/// The file every skill declares itself in.
const MANIFEST: &str = "SKILL.md.tmpl";

/// One skill, as authored.
#[derive(Debug, Clone)]
pub struct Skill {
    /// The skill's name, which is its directory.
    pub name: String,
    /// What it declares about itself.
    pub meta: SkillMeta,
    /// Where it is authored.
    pub dir:  PathBuf,
}

/// Everything under one `.shared/skills/` directory.
#[derive(Debug, Clone, Default)]
pub struct Skills {
    pub skills: Vec<Skill>,
}

/// Why a skill corpus could not be read or rendered.
#[derive(Debug)]
pub enum SkillsError {
    /// The directory does not exist or could not be listed.
    Unreadable {
        path:  PathBuf,
        cause: io::Error,
    },
    /// A directory under the corpus declares no `SKILL.md.tmpl`.
    ///
    /// Refused rather than skipped: a directory sitting in the corpus that
    /// generates nothing is either a skill somebody forgot to finish or a stray,
    /// and both want saying out loud.
    NoManifest {
        path: PathBuf,
    },
    /// One skill's frontmatter is wrong, named with its file.
    Meta {
        path:  PathBuf,
        cause: SkillError,
    },
    /// The declared name and the directory disagree.
    NameMismatch {
        path:     PathBuf,
        declared: String,
        dir:      String,
    },
    /// A template was authored but does not render.
    Render {
        path:   PathBuf,
        reason: String,
    },
    /// Writing a generated file failed.
    Write {
        path:  PathBuf,
        cause: io::Error,
    },
}

impl std::fmt::Display for SkillsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable {
                path,
                cause,
            } => write!(f, "{}: {cause}", path.display()),
            Self::NoManifest {
                path,
            } => {
                write!(
                    f,
                    "{}: no {MANIFEST}, so this directory is in the corpus and generates nothing",
                    path.display()
                )
            },
            Self::Meta {
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
                    "{}: declares the name `{declared}` and sits in `{dir}`, so it is documented \
                     under one name and invoked under another",
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

impl std::error::Error for SkillsError {}

/// What a skill's templates render against.
#[derive(Debug, Serialize)]
struct Ctx<'a> {
    name:        &'a str,
    description: &'a str,
}

impl Skills {
    /// Read every skill under `dir`.
    pub fn load(dir: &Path) -> Result<Self, SkillsError> {
        let entries = fs::read_dir(dir).map_err(|cause| {
            SkillsError::Unreadable {
                path: dir.to_path_buf(),
                cause,
            }
        })?;

        // Sorted by name, so the order the corpus is walked in does not depend
        // on the order the filesystem happens to list it in.
        let mut dirs: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in entries {
            let entry = entry.map_err(|cause| {
                SkillsError::Unreadable {
                    path: dir.to_path_buf(),
                    cause,
                }
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            dirs.insert(name.to_string(), path);
        }

        let mut skills = Vec::with_capacity(dirs.len());
        for (name, path) in dirs {
            let manifest = path.join(MANIFEST);
            if !manifest.is_file() {
                return Err(SkillsError::NoManifest {
                    path,
                });
            }
            let source = fs::read_to_string(&manifest).map_err(|cause| {
                SkillsError::Unreadable {
                    path: manifest.clone(),
                    cause,
                }
            })?;
            let parsed = skill::parse(&source).map_err(|cause| {
                SkillsError::Meta {
                    path: manifest.clone(),
                    cause,
                }
            })?;
            if !skill::name_matches_dir(&parsed.meta, &name) {
                return Err(SkillsError::NameMismatch {
                    path:     manifest,
                    declared: parsed.meta.name,
                    dir:      name,
                });
            }
            skills.push(Skill {
                name,
                meta: parsed.meta,
                dir: path,
            });
        }
        Ok(Self {
            skills,
        })
    }

    /// Render every skill into `dst`, returning the files written.
    ///
    /// Each skill's generated directory is removed first, so a file dropped
    /// from the authored side does not survive in the generated one. A
    /// directory in `dst` that no authored skill claims is left alone and
    /// reported by [`Self::unclaimed`], since deleting what this pass did not
    /// write is a decision for whoever put it there.
    pub fn render(&self, dst: &Path) -> Result<Vec<PathBuf>, SkillsError> {
        fs::create_dir_all(dst).map_err(|cause| {
            SkillsError::Write {
                path: dst.to_path_buf(),
                cause,
            }
        })?;

        let mut written = Vec::new();
        for s in &self.skills {
            let out = dst.join(&s.name);
            if out.exists() {
                fs::remove_dir_all(&out).map_err(|cause| {
                    SkillsError::Write {
                        path: out.clone(),
                        cause,
                    }
                })?;
            }
            self.render_tree(s, &s.dir, &out, &mut written)?;
        }
        Ok(written)
    }

    /// One directory of one skill, recursively.
    fn render_tree(
        &self,
        s: &Skill,
        src: &Path,
        dst: &Path,
        written: &mut Vec<PathBuf>,
    ) -> Result<(), SkillsError> {
        fs::create_dir_all(dst).map_err(|cause| {
            SkillsError::Write {
                path: dst.to_path_buf(),
                cause,
            }
        })?;

        let entries = fs::read_dir(src).map_err(|cause| {
            SkillsError::Unreadable {
                path: src.to_path_buf(),
                cause,
            }
        })?;

        // Collected and sorted so the written list is stable between runs,
        // which is what lets a caller diff two of them.
        //
        // The kind comes from `file_type`, which does not follow a symlink,
        // rather than from `is_dir`, which does. A skill may hold a link to a
        // sibling repository, and following one copies that whole repository
        // into the generated tree: measured at 944 files from a single link
        // into `nutshell`, against the 9 files that skill actually owns.
        let mut paths: Vec<(PathBuf, fs::FileType)> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|cause| {
                SkillsError::Unreadable {
                    path: src.to_path_buf(),
                    cause,
                }
            })?;
            let kind = entry.file_type().map_err(|cause| {
                SkillsError::Unreadable {
                    path: entry.path(),
                    cause,
                }
            })?;
            paths.push((entry.path(), kind));
        }
        paths.sort_by(|a, b| a.0.cmp(&b.0));

        for (path, kind) in paths {
            let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if kind.is_symlink() {
                let target = dst.join(file);
                copy_link(&path, &target)?;
                written.push(target);
                continue;
            }
            if kind.is_dir() {
                self.render_tree(s, &path, &dst.join(file), written)?;
                continue;
            }
            match file.strip_suffix(TMPL_SUFFIX) {
                Some(rendered_name) => {
                    let source = fs::read_to_string(&path).map_err(|cause| {
                        SkillsError::Unreadable {
                            path: path.clone(),
                            cause,
                        }
                    })?;
                    let out = self.render_one(s, &source, &path)?;
                    let target = dst.join(rendered_name);
                    // A trailing newline, because a file without one
                    // concatenates with whatever follows it.
                    fs::write(&target, format!("{}\n", out.trim_end())).map_err(|cause| {
                        SkillsError::Write {
                            path: target.clone(),
                            cause,
                        }
                    })?;
                    written.push(target);
                },
                None => {
                    let target = dst.join(file);
                    fs::copy(&path, &target).map_err(|cause| {
                        SkillsError::Write {
                            path: target.clone(),
                            cause,
                        }
                    })?;
                    copy_mode(&path, &target)?;
                    written.push(target);
                },
            }
        }
        Ok(())
    }

    /// Render one template against its skill's own meta.
    fn render_one(&self, s: &Skill, source: &str, path: &Path) -> Result<String, SkillsError> {
        // A fresh environment per render rather than one shared: strict
        // undefined handling is the point, and a shared registry would let a
        // template resolve a name some other skill happened to define.
        let env = TemplateEnv::new();
        let ctx = Ctx {
            name:        &s.name,
            description: &s.meta.description,
        };
        env.render_str(source, &ctx).map_err(|e| {
            SkillsError::Render {
                path:   path.to_path_buf(),
                reason: e.to_string(),
            }
        })
    }

    /// Directories in `dst` that no authored skill claims.
    ///
    /// Reported rather than removed. One is either a skill that has not been
    /// moved to the authored side yet or something else entirely, and this pass
    /// cannot tell which.
    pub fn unclaimed(&self, dst: &Path) -> Result<Vec<String>, SkillsError> {
        if !dst.is_dir() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(dst).map_err(|cause| {
            SkillsError::Unreadable {
                path: dst.to_path_buf(),
                cause,
            }
        })?;
        let mut stray = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|cause| {
                SkillsError::Unreadable {
                    path: dst.to_path_buf(),
                    cause,
                }
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            if !self.skills.iter().any(|s| s.name == name) {
                stray.push(name.to_string());
            }
        }
        stray.sort();
        Ok(stray)
    }

    /// The skills that cite a rule by name, in the two forms a citation takes.
    ///
    /// **A backticked filename or a `[[link]]`, never a bare word.** The bare
    /// form matches prose that happens to use the same words as a rule's title,
    /// which on a corpus this size is most of them.
    pub fn bearing_on(&self, rule: &str) -> Result<Vec<String>, SkillsError> {
        let filename = format!("`{rule}.md`");
        let link = format!("[[{rule}]]");
        let mut hits = Vec::new();
        for s in &self.skills {
            if cites(&s.dir, &filename, &link)? {
                hits.push(s.name.clone());
            }
        }
        Ok(hits)
    }
}

/// Whether anything under `dir` carries either citation form.
fn cites(dir: &Path, filename: &str, link: &str) -> Result<bool, SkillsError> {
    let entries = fs::read_dir(dir).map_err(|cause| {
        SkillsError::Unreadable {
            path: dir.to_path_buf(),
            cause,
        }
    })?;
    for entry in entries {
        let entry = entry.map_err(|cause| {
            SkillsError::Unreadable {
                path: dir.to_path_buf(),
                cause,
            }
        })?;
        let path = entry.path();
        if path.is_dir() {
            if cites(&path, filename, link)? {
                return Ok(true);
            }
            continue;
        }
        // Read as bytes and lose what is not text: a skill may carry a binary
        // and a citation cannot be inside one.
        let Ok(body) = fs::read_to_string(&path) else {
            continue;
        };
        if body.contains(filename) || body.contains(link) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Recreate a symlink rather than following it.
///
/// The link's own text is copied, so a relative one keeps pointing at the same
/// place: the authored and generated trees sit at the same depth under the
/// workspace root, which is what makes that true and is worth knowing before
/// either moves.
#[cfg(unix)]
fn copy_link(src: &Path, dst: &Path) -> Result<(), SkillsError> {
    let target = fs::read_link(src).map_err(|cause| {
        SkillsError::Unreadable {
            path: src.to_path_buf(),
            cause,
        }
    })?;
    std::os::unix::fs::symlink(&target, dst).map_err(|cause| {
        SkillsError::Write {
            path: dst.to_path_buf(),
            cause,
        }
    })
}

/// Where symlinks are not available, the link is skipped and said so.
#[cfg(not(unix))]
fn copy_link(src: &Path, _dst: &Path) -> Result<(), SkillsError> {
    Err(SkillsError::Write {
        path:  src.to_path_buf(),
        cause: io::Error::new(
            io::ErrorKind::Unsupported,
            "a skill carries a symlink and this platform cannot recreate one",
        ),
    })
}

/// Carry a file's permissions across a copy.
#[cfg(unix)]
fn copy_mode(src: &Path, dst: &Path) -> Result<(), SkillsError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(src)
        .map_err(|cause| {
            SkillsError::Unreadable {
                path: src.to_path_buf(),
                cause,
            }
        })?
        .permissions()
        .mode();
    fs::set_permissions(dst, fs::Permissions::from_mode(mode)).map_err(|cause| {
        SkillsError::Write {
            path: dst.to_path_buf(),
            cause,
        }
    })
}

/// Nothing to carry where the platform has no mode bits.
#[cfg(not(unix))]
fn copy_mode(_src: &Path, _dst: &Path) -> Result<(), SkillsError> {
    Ok(())
}
