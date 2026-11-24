//! Which indexes the workspace holds, and what each is called.
//!
//! The content repository's corpora are the corpus config files it tracks, each naming its own index
//! or taking the default beside it. A member's index is under its own git directory, asked only where
//! `[search]` says the members are.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use homma_api::SearchConfig;
use homma_api::path::RelPath;
use homma_core::Config;

/// One index the question can be put to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// What its hits are shown under.
    pub name:  String,
    /// The index file.
    pub index: PathBuf,
    /// Why it is not asked at all, where finding it already failed.
    pub shut:  Option<String>,
    /// What was found wrong on the way to it that still left it askable.
    pub notes: Vec<String>,
}

impl Place {
    /// A place asked at `index`, with nothing said about it.
    #[must_use]
    pub const fn at(name: String, index: PathBuf) -> Self {
        Self {
            name,
            index,
            shut: None,
            notes: Vec::new(),
        }
    }
}

/// The `[search]` table, with its defaults where it is absent, and the members asked as well where
/// `members` says so for this one run, whatever the table says.
///
/// # Errors
/// A table that does not read as one.
pub fn search_config(cfg: &Config, members: bool) -> Result<SearchConfig> {
    let mut read: SearchConfig = match &cfg.search {
        Some(v) => v.clone().try_into().context("reading `[search]`")?,
        None => SearchConfig::default(),
    };
    read.members |= members;
    Ok(read)
}

/// Every index of the workspace: the corpora of the content repository at `root`, named by
/// `content` and the directory below it, then each member's in the order given where `members` is
/// set, built or not, so one that is not is a line saying so.
///
/// # Errors
/// Git's words where the content repository's files cannot be listed.
pub fn places(
    root: &Path,
    content: &str,
    search: &SearchConfig,
    members: impl IntoIterator<Item = (String, PathBuf)>,
) -> Result<Vec<Place>> {
    let mut out = corpora(root, content, search)?;
    if search.members {
        for (name, dir) in members {
            out.push(match member_index(&dir, search) {
                Ok(index) => Place::at(name, index),
                Err(why) => {
                    Place {
                        shut: Some(why),
                        ..Place::at(name, dir)
                    }
                },
            });
        }
    }
    Ok(out)
}

/// The places a question names, or every one where it names none.
///
/// # Errors
/// A name no place carries, with the names there are.
pub fn narrowed(places: Vec<Place>, only: &[String]) -> Result<Vec<Place>> {
    if only.is_empty() {
        return Ok(places);
    }
    if let Some(unknown) = only.iter().find(|n| !places.iter().any(|p| &&p.name == n)) {
        let known: Vec<&str> = places.iter().map(|p| p.name.as_str()).collect();
        return Err(anyhow!(
            "no index is named `{unknown}`; the indexes are: {}",
            known.join(", ")
        ));
    }
    Ok(places
        .into_iter()
        .filter(|p| only.contains(&p.name))
        .collect())
}

/// The corpus config files the content repository tracks, as corpora.
fn corpora(root: &Path, content: &str, search: &SearchConfig) -> Result<Vec<Place>> {
    let wanted = search.corpus_config.as_path();
    let spec = format!(":(glob)**/{}", wanted.display());
    let listed = git(root, &["ls-files", "-z", "--", spec.as_str()])?;
    let mut out = Vec::new();
    for rel in listed.split('\0').filter(|s| !s.is_empty()) {
        let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
        let name = if dir.as_os_str().is_empty() {
            content.to_string()
        } else {
            format!("{content}/{}", dir.display())
        };
        let mut notes = Vec::new();
        let named = match own_index(&root.join(rel)) {
            Ok(named) => named,
            Err(why) => {
                notes.push(why);
                None
            },
        };
        let index = named.unwrap_or_else(|| search.corpus_index.as_path().to_path_buf());
        // The index is the config's own to place, inside the content repository and nowhere else.
        let place = match RelPath::new(dir.join(&index)) {
            Ok(inside) => Place::at(name, root.join(inside.as_path())),
            Err(_) => {
                Place {
                    shut: Some(format!(
                        "`corpus.index = \"{}\"` in {rel} leaves the content repository, so it is \
                         not opened",
                        index.display()
                    )),
                    ..Place::at(name, index)
                }
            },
        };
        out.push(Place {
            notes,
            ..place
        });
    }
    Ok(out)
}

/// The index a corpus config names for itself, `corpus.index`, where it names one, or why the
/// config could not be read for it.
fn own_index(config: &Path) -> Result<Option<PathBuf>, String> {
    let unread = |e: &dyn std::fmt::Display| {
        format!(
            "{} does not read ({e}), so its index is the default",
            config.display()
        )
    };
    let text = std::fs::read_to_string(config).map_err(|e| unread(&e))?;
    let value: toml::Value = toml::from_str(&text).map_err(|e| unread(&e))?;
    match value.get("corpus").and_then(|c| c.get("index")) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(PathBuf::from(s))),
        Some(_) => Err(unread(&"`corpus.index` is not a string")),
    }
}

/// Where a member's index is, under its own git directory, or why there is no git directory.
fn member_index(dir: &Path, search: &SearchConfig) -> Result<PathBuf, String> {
    if !dir.is_dir() {
        return Err(format!("no clone at {}", dir.display()));
    }
    let git_dir = git(dir, &["rev-parse", "--absolute-git-dir"]).map_err(|e| e.to_string())?;
    Ok(PathBuf::from(git_dir.trim()).join(search.member_index.as_path()))
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| format!("running git in {}", dir.display()))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {} in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What the content repository's corpora are named under: the last segment of `content_repo`
/// without `.git`, or the root's directory where it is unset or means the root itself.
#[must_use]
pub fn content_name(content_repo: Option<&str>, root: &Path) -> String {
    let named = content_repo
        .filter(|c| *c != homma_api::config::LOCAL)
        .and_then(|c| c.trim_end_matches('/').rsplit(['/', ':']).next())
        .map(|s| s.trim_end_matches(".git"))
        .filter(|s| !s.is_empty());
    match named {
        Some(n) => n.to_string(),
        None => {
            root.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        },
    }
}
