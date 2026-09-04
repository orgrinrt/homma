//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma workspace`, the one command the launcher answers without the engine.
//!
//! It runs where there is no workspace yet, which is the whole reason it is
//! the launcher's: the engine needs a `homma.toml` above the cwd to run at
//! all, and making that workspace is what this does. Bare, it reports the
//! workspace the cwd is in or makes one in place; `spawn`, `reap` and `list`
//! are the forms that name one.
//!
//! This is the session's throwaway clone and knows nothing about identities.
//! The org design's workspace lifecycle, which stands a participant's
//! workspace up from a registry entry inside a root, is the engine's and
//! stays there.

pub mod git;
pub mod reap;
pub mod spawn;
pub mod status;

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use renki::{Command, Invocation};

use crate::settings::Prefs;

/// The table the descriptor carries.
pub const COMMANDS: &[Command] = &[Command {
    name: "workspace",
    doc: "Report the workspace the cwd is in, or make one; spawn, reap and list by name.",
    run,
}];

/// What the arguments asked for, parsed once so the verbs are one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Bare: status inside a workspace, spawn in place outside one.
    Bare,
    /// `spawn <slug> [owner/name ...] [--branch <name>]`.
    Spawn {
        slug:   String,
        repos:  Vec<String>,
        branch: Option<String>,
    },
    /// `reap [<slug>] [--force]`.
    Reap {
        slug:  Option<String>,
        force: bool,
    },
    /// `list`.
    List,
}

/// The usage line, printed on a verb or flag this does not know.
pub const USAGE: &str = "homma workspace [spawn <slug> [owner/name ...] [--branch <name>] | reap [<slug>] [--force] | list]";

impl Ask {
    /// Parse the arguments after `workspace`.
    pub fn parse(args: &[OsString]) -> Result<Ask, String> {
        let text: Vec<&str> = args
            .iter()
            .map(|a| {
                a.to_str()
                    .ok_or_else(|| format!("{a:?} is not text; {USAGE}"))
            })
            .collect::<Result<_, _>>()?;
        match text.split_first() {
            None => Ok(Ask::Bare),
            Some((&"list", rest)) => {
                if rest.is_empty() {
                    Ok(Ask::List)
                } else {
                    Err(format!("list takes nothing after it; {USAGE}"))
                }
            },
            Some((&"reap", rest)) => {
                let mut slug = None;
                let mut force = false;
                for a in rest {
                    match *a {
                        "--force" => force = true,
                        s if s.starts_with('-') => {
                            return Err(format!("unknown flag {s}; {USAGE}"));
                        },
                        s if slug.is_none() => slug = Some(s.to_owned()),
                        _ => return Err(format!("reap takes one slug at a time; {USAGE}")),
                    }
                }
                Ok(Ask::Reap {
                    slug,
                    force,
                })
            },
            Some((&"spawn", rest)) => {
                let mut slug = None;
                let mut repos = Vec::new();
                let mut branch = None;
                let mut it = rest.iter();
                while let Some(a) = it.next() {
                    match *a {
                        "--branch" => {
                            let b = it
                                .next()
                                .ok_or_else(|| format!("--branch needs a name; {USAGE}"))?;
                            branch = Some((*b).to_owned());
                        },
                        s if s.starts_with('-') => {
                            return Err(format!("unknown flag {s}; {USAGE}"));
                        },
                        s if slug.is_none() => slug = Some(s.to_owned()),
                        s => repos.push(s.to_owned()),
                    }
                }
                let slug = slug.ok_or_else(|| format!("spawn needs a slug; {USAGE}"))?;
                check_slug(&slug)?;
                Ok(Ask::Spawn {
                    slug,
                    repos,
                    branch,
                })
            },
            Some((other, _)) => Err(format!("unknown verb {other}; {USAGE}")),
        }
    }
}

/// A slug is one directory name: no separator, not `.` or `..`, not empty.
pub fn check_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug == "." || slug == ".." || slug.contains('/') {
        return Err(format!(
            "a slug is a single directory name, and {slug:?} is not"
        ));
    }
    Ok(())
}

fn run(inv: &Invocation<'_>) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let prefs = Prefs::read(inv, home.as_deref())?;
    let out = std::io::stdout();
    let mut out = out.lock();
    answer(
        &prefs,
        home.as_deref(),
        inv.cwd(),
        inv.root(),
        Ask::parse(inv.args())?,
        &mut out,
    )
}

/// The command, over what it needs and nothing process-global, so a test
/// hands in a temporary home and reads what was printed.
pub fn answer(
    prefs: &Prefs,
    home: Option<&Path>,
    cwd: &Path,
    root: Option<&Path>,
    ask: Ask,
    out: &mut dyn Write,
) -> Result<(), String> {
    match ask {
        Ask::Bare => {
            match root {
                Some(root) => status::report(root, out),
                None => spawn::in_place(prefs, home, cwd, out),
            }
        },
        Ask::Spawn {
            slug,
            repos,
            branch,
        } => spawn::under_root(prefs, home, &slug, &repos, branch.as_deref(), out),
        Ask::Reap {
            slug,
            force,
        } => {
            match slug {
                Some(slug) => {
                    check_slug(&slug)?;
                    reap::reap(
                        prefs,
                        home,
                        cwd,
                        &prefs.workspaces_root.join(slug),
                        force,
                        out,
                    )
                },
                None => {
                    let root = root.ok_or(
                        "the cwd is not inside a workspace, so `reap` needs a slug to say which one",
                    )?;
                    reap::reap(prefs, home, cwd, root, force, out)
                },
            }
        },
        Ask::List => reap::list(&prefs.workspaces_root, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_verbs_parse_and_the_rest_is_refused_by_name() {
        assert_eq!(Ask::parse(&s(&[])).unwrap(), Ask::Bare);
        assert_eq!(Ask::parse(&s(&["list"])).unwrap(), Ask::List);
        assert_eq!(Ask::parse(&s(&["reap"])).unwrap(), Ask::Reap {
            slug:  None,
            force: false,
        });
        assert_eq!(
            Ask::parse(&s(&["reap", "alpha", "--force"])).unwrap(),
            Ask::Reap {
                slug:  Some("alpha".into()),
                force: true,
            }
        );
        assert_eq!(
            Ask::parse(&s(&[
                "spawn", "alpha", "o/notko", "--branch", "feat/x", "o/arvo"
            ]))
            .unwrap(),
            Ask::Spawn {
                slug:   "alpha".into(),
                repos:  vec!["o/notko".into(), "o/arvo".into()],
                branch: Some("feat/x".into()),
            }
        );
        for bad in [
            vec!["status"],
            vec!["list", "x"],
            vec!["reap", "a", "b"],
            vec!["reap", "--nope"],
            vec!["spawn"],
            vec!["spawn", "--branch"],
            vec!["spawn", "a/b"],
            vec!["spawn", ".."],
            vec!["spawn", "a", "--nope"],
        ] {
            let err = Ask::parse(&s(&bad)).expect_err(&format!("{bad:?} parsed"));
            assert!(
                err.contains("homma workspace") || err.contains("slug"),
                "{err}"
            );
        }
    }
}
