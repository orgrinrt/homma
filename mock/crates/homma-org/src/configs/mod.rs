//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The tool configs every repo is meant to share, and whether it has them.
//!
//! `rustfmt.toml`, `deny.toml` and whatever joins them are the same file in
//! every repo that has one. What differs between repos is only whether one got
//! copied, and a repo without a `rustfmt.toml` formats to rustfmt's defaults
//! rather than ours, which surfaces months later as noise inside somebody's
//! unrelated diff.
//!
//! So the canonical copies live in one place and this compares against them.
//! Which repos want a given one is a property of that config, so it travels
//! with it, as the directory it sits in. [`tags`] carries that half.
//!
//! Absence is acted on and difference is not. A missing config is a fact:
//! nothing was decided, and placing the template is what somebody would have
//! done by hand. A config that differs is a question this module cannot answer,
//! because a deliberate exception and a drifted copy look identical on disk. It
//! is reported and left exactly as it is.
//!
//! There is no way for a repo to write down that it does not want a config it
//! is otherwise in the set for, and that is deliberate. A derived refusal is
//! worked out again on every run, so it is right whenever the repo changes and
//! needs nobody to revisit it: a repo that cannot take a nightly-only config
//! says so by not pinning a nightly. A written one is remembered instead, and
//! goes on claiming what it claimed after that stops being true, with nothing
//! able to tell, because the thing it asserts is exactly the thing nobody
//! rechecks. Where a repo needs a refusal that is not derivable, the fix is a
//! new giveaway or a new tag, both of which are data.

pub mod tags;

use std::path::{Path, PathBuf};

use homma_api::{ContainedPath, Root};
pub use tags::{Ecosystem, Severity, Tag};

/// Where the canonical copies live, relative to the workspace root.
pub const CONFIGS_DIR: &str = ".shared/configs";

/// One canonical config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// The name it deploys as, which is also the name it is stored under. No
    /// mapping, so there is nothing to drift.
    pub file_name: String,
    /// Which repos want it, and how hard a rule it is for each.
    ///
    /// Empty is a template nobody has said where to put. It is loaded anyway,
    /// so it can be reported rather than passed over silently.
    pub tags:      Vec<Tag>,
    /// The bytes, exactly as they deploy. Nothing is stripped on the way, so
    /// comparing a repo's copy against this stays a comparison of bytes.
    pub body:      Vec<u8>,
}

impl Template {
    /// The paths in `repo_dir` that count as having this config.
    ///
    /// The root, then `mock/`. Not invented for this: both giveaway predicates
    /// in [`tags`] already look in exactly those two places, so this is the
    /// rule they were each spelling out separately. A repo whose Cargo
    /// workspace lives under `mock/` may keep its config only there, and it is
    /// not missing anything.
    pub fn satisfying_paths(&self, repo_dir: &Path) -> [PathBuf; 2] {
        [repo_dir.join(&self.file_name), repo_dir.join("mock").join(&self.file_name)]
    }

    /// Where the copy is, if there is one.
    pub fn found_at(&self, repo_dir: &Path) -> Option<PathBuf> {
        self.satisfying_paths(repo_dir)
            .into_iter()
            .find(|p| p.is_file())
    }

    /// The severity this template carries for the repo at `dir`, or `None`
    /// where the repo is in none of its sets.
    ///
    /// The strongest among the sets the repo is actually in. A config that is
    /// required of Rust repos and suggested for deno ones is required of a repo
    /// that is both, because the stronger claim is the one somebody made.
    pub fn severity_for(&self, dir: &Path) -> Option<Severity> {
        self.tags
            .iter()
            .filter(|t| t.ecosystem.wants(dir))
            .map(|t| t.severity)
            .max()
    }

    /// Whether the repo is in the wider set of one of this template's tags
    /// while not being in the tagged set itself.
    ///
    /// That repo is not one the template passes over. It is one the template
    /// cannot serve, and the difference matters: passing over a deno repo is
    /// right, while passing over a stable Rust repo leaves the only Rust repo
    /// in a workspace with no formatting config and no sign of it.
    pub fn is_a_near_miss(&self, dir: &Path) -> bool {
        self.tags
            .iter()
            .filter_map(|t| t.ecosystem.base())
            .any(|base| base.wants(dir))
    }
}

/// Reading `.shared/configs/` failed.
#[derive(Debug)]
pub enum TemplateError {
    /// The directory is not there.
    Missing(PathBuf),
    /// It is there and could not be read.
    Io(PathBuf, std::io::Error),
    /// A tag directory names something that is not an ecosystem, or names one
    /// twice.
    BadTag(String, tags::TagsError),
    /// One file name appears under two tag directories.
    ///
    /// Refused rather than resolved. Two answers for one config is a
    /// contradiction, and picking either would be this deciding something
    /// nobody wrote down.
    Conflict(String, String, String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(p) => {
                write!(
                    f,
                    "no shared configs at {}; nothing to compare a repo against",
                    p.display()
                )
            },
            Self::Io(p, e) => write!(f, "reading {}: {e}", p.display()),
            Self::BadTag(dir, e) => write!(f, "the tag directory `{dir}`: {e}"),
            Self::Conflict(name, a, b) => {
                write!(
                    f,
                    "`{name}` is under both `{a}` and `{b}`; which one applies is unstated"
                )
            },
        }
    }
}

impl std::error::Error for TemplateError {}

/// Every canonical config under `dir`, which is a workspace root.
///
/// One level of subdirectories, each naming the tags its contents carry. A file
/// loose at the top level is untagged, which is a config nobody has said where
/// to put: it is loaded so it can be reported.
///
/// `README.md` is documentation for the directory rather than a config and is
/// skipped by name, at either level.
pub fn templates(dir: &Path) -> Result<Vec<Template>, TemplateError> {
    let at = dir.join(CONFIGS_DIR);
    if !at.is_dir() {
        return Err(TemplateError::Missing(at));
    }
    let mut out: Vec<(Template, String)> = Vec::new();
    for entry in read_dir(&at)? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        if name == "README.md" {
            continue;
        }
        if path.is_dir() {
            let tags =
                tags::Tag::parse_dir(&name).map_err(|e| TemplateError::BadTag(name.clone(), e))?;
            for inner in read_dir(&path)? {
                let inner_path = inner.path();
                if !inner_path.is_file() {
                    continue;
                }
                let Some(file_name) = inner_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                else {
                    continue;
                };
                if file_name == "README.md" {
                    continue;
                }
                push(&mut out, &inner_path, file_name, tags.clone(), name.clone())?;
            }
        } else if path.is_file() {
            push(&mut out, &path, name, Vec::new(), String::from("."))?;
        }
    }
    out.sort_by(|a, b| a.0.file_name.cmp(&b.0.file_name));
    Ok(out.into_iter().map(|(t, _)| t).collect())
}

fn read_dir(at: &Path) -> Result<Vec<std::fs::DirEntry>, TemplateError> {
    let entries = std::fs::read_dir(at).map_err(|e| TemplateError::Io(at.to_path_buf(), e))?;
    entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TemplateError::Io(at.to_path_buf(), e))
}

fn push(
    out: &mut Vec<(Template, String)>,
    path: &Path,
    file_name: String,
    tags: Vec<Tag>,
    from: String,
) -> Result<(), TemplateError> {
    if let Some((_, other)) = out.iter().find(|(t, _)| t.file_name == file_name) {
        return Err(TemplateError::Conflict(file_name, other.clone(), from));
    }
    let body = std::fs::read(path).map_err(|e| TemplateError::Io(path.to_path_buf(), e))?;
    out.push((
        Template {
            file_name,
            tags,
            body,
        },
        from,
    ));
    Ok(())
}

/// What was found for one config in one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// It is there and is the canonical copy.
    Matches(String),
    /// It is there and is something else. Left alone, because the difference
    /// may be meant and this cannot tell.
    Differs(String),
    /// The repo wants it and does not have it.
    Missing(String, Severity),
    /// It was missing and now is not. Only [`ensure`] produces this.
    Placed(String),
    /// It is missing and where it belongs is not known, so a human places it.
    CannotInfer(String),
    /// The repo is in the wider set but not the tagged one, and has no copy.
    /// The variant that would fit has not been written, so a human writes it.
    /// Carries what does not fit.
    NoVariantFits(String, String),
    /// It is missing and placing it did not work.
    Failed(String, String),
}

impl Finding {
    /// The config this is about.
    pub fn file_name(&self) -> &str {
        match self {
            Self::Matches(n)
            | Self::Differs(n)
            | Self::Missing(n, _)
            | Self::Placed(n)
            | Self::CannotInfer(n)
            | Self::NoVariantFits(n, _)
            | Self::Failed(n, _) => n,
        }
    }

    /// Whether this stops a commit.
    ///
    /// Only a missing required config. Everything else reports and lets the
    /// commit through, and each for its own reason.
    ///
    /// A difference does not, because blocking would mean the only way forward
    /// is to overwrite it, which makes this a worse version of doing that on
    /// purpose. An unplaceable template does not, because that is a fault in
    /// the shared directory rather than in the repo, and refusing a repo's
    /// commits over the workspace's own configuration punishes the wrong party.
    /// A near miss does not, because the variant that would fit has not been
    /// written and nobody in that repo can write it from there.
    pub fn blocks(&self) -> bool {
        matches!(self, Self::Missing(_, Severity::Required))
    }

    /// Whether this is something an operator has to act on.
    ///
    /// A difference is not one: a warning is not an error, and a tool that
    /// refuses to run over a workspace whose configs are fine is a tool
    /// somebody switches off.
    pub fn needs_a_human(&self) -> bool {
        matches!(
            self,
            Self::CannotInfer(_) | Self::NoVariantFits(..) | Self::Failed(..)
        ) || self.blocks()
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Matches(n) => write!(f, "{n} matches"),
            Self::Differs(n) => write!(f, "{n} differs from the shared copy, left as it is"),
            Self::Missing(n, s) => write!(f, "{n} is missing, and is {s} here"),
            Self::Placed(n) => write!(f, "placed {n}"),
            Self::CannotInfer(n) => {
                write!(f, "{n} is not there and nothing says which repos want it")
            },
            Self::NoVariantFits(n, why) => {
                write!(
                    f,
                    "{n} is not there and the shared copy does not fit: {why}"
                )
            },
            Self::Failed(n, e) => write!(f, "could not place {n}: {e}"),
        }
    }
}

/// Compare one repo against the canonical configs, writing nothing.
///
/// This is what the commit path runs. It is separate from [`ensure`] because
/// placing a config turns a check on, and a check that was not running has not
/// been passing: the consequences land wherever that tool looks, which is
/// routinely somewhere nobody had in mind. A gate that ran on every commit and
/// wrote into the tree it inspects would hand somebody that at the worst
/// possible moment.
pub fn inspect(repo_dir: &Path, templates: &[Template]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for t in templates {
        let found = t.found_at(repo_dir);
        if t.tags.is_empty() {
            // Reported once per repo rather than once per workspace, because a
            // template with no home is a question about every repo equally and
            // the operator is looking at a per-repo table.
            if found.is_none() {
                findings.push(Finding::CannotInfer(t.file_name.clone()));
            }
            continue;
        }
        let Some(severity) = t.severity_for(repo_dir) else {
            if found.is_none() && t.is_a_near_miss(repo_dir) {
                findings.push(Finding::NoVariantFits(
                    t.file_name.clone(),
                    "it is tagged for a narrower set of repos than this one is in".into(),
                ));
            }
            continue;
        };
        findings.push(match found {
            Some(at) => {
                match std::fs::read(&at) {
                    Ok(have) if have == t.body => Finding::Matches(t.file_name.clone()),
                    Ok(_) => Finding::Differs(t.file_name.clone()),
                    Err(e) => Finding::Failed(t.file_name.clone(), e.to_string()),
                }
            },
            None => Finding::Missing(t.file_name.clone(), severity),
        });
    }
    findings
}

/// Compare one repo against the canonical configs, placing what is missing.
///
/// `repo_dir` is contained under `root` already, which is what makes a
/// placement a contained write rather than a bare `std::fs` one.
///
/// Built on [`inspect`] rather than beside it, so the two cannot come to
/// different answers about what a repo is missing.
pub fn ensure(root: &Root, repo_dir: &ContainedPath, templates: &[Template]) -> Vec<Finding> {
    inspect(repo_dir.as_path(), templates)
        .into_iter()
        .map(|f| {
            match f {
                Finding::Missing(name, _) => {
                    let Some(t) = templates.iter().find(|t| t.file_name == name) else {
                        return Finding::Failed(name, "no such template".into());
                    };
                    let placed = root
                        .contain_under(repo_dir, &name)
                        .map_err(|e| e.to_string())
                        .and_then(|p| root.write(&p, &t.body).map_err(|e| e.to_string()));
                    match placed {
                        Ok(()) => Finding::Placed(name),
                        Err(e) => Finding::Failed(name, e),
                    }
                },
                other => other,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
