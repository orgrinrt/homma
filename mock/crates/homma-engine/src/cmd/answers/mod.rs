//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `homma answers take|delegate|hook`: what was left on a page or a channel, into the capture
//! store and onto the rows it answers.
//!
//! `take` reads answer and note documents from a directory or a channel, writes one capture of
//! what is new, and attaches each answer to its row as `<at>: <capture path>` through
//! mockspace's `registry-append`. It prints nothing when nothing was new and one line when
//! something was. `delegate` is the program a workspace's `.mockspace/hooks/registry-append`
//! runs, and `hook` places or removes that file. Committing what these write is the caller's.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

use self::docs::{Collection, Read};
use crate::cli::{AnswersOp, Cli, OutputFormat};
use crate::cmd::{Outcome, config_path, load_config};
use crate::output::{HumanRender, emit};

pub mod attach;
pub mod delegate;
pub mod docs;
pub mod hook;
pub mod render;

#[cfg(test)]
mod tests;

/// Where the documents came from, as the capture says it.
pub enum Source<'a> {
    Dir(&'a Path),
    Channel {
        channel: &'a str,
        from:    &'a str,
    },
    Delegated(&'a str),
}

impl Source<'_> {
    fn words(&self) -> String {
        match self {
            Source::Dir(d) => format!("the documents in {}", d.display()),
            Source::Channel {
                channel,
                ..
            } => format!("the channel {channel}"),
            Source::Delegated(t) => format!("registry-append of {t}"),
        }
    }

    /// The tag a capture carries for where its words were left.
    pub(crate) fn tag(&self) -> &'static str {
        match self {
            Source::Dir(_) => "dashboard",
            Source::Channel {
                ..
            } => "channel",
            Source::Delegated(_) => "registry-append",
        }
    }

    /// The sentence under a capture's title.
    pub(crate) fn lead(&self) -> String {
        match self {
            Source::Dir(_) => "Left on the dashboard page, copied field for field.".into(),
            Source::Channel {
                channel,
                ..
            } => {
                format!("Left on the channel {channel}, copied field for field.")
            },
            Source::Delegated(t) => {
                format!("Appended to {t} with registry-append, copied field for field.")
            },
        }
    }

    /// Who the title says spoke: the page is op's, a channel's is whoever `--from` names, and
    /// a delegated append does not say.
    pub(crate) fn speaker(&self) -> &str {
        match self {
            Source::Dir(_) => "op",
            Source::Channel {
                from,
                ..
            } => from,
            Source::Delegated(_) => "someone",
        }
    }

    /// Where a title says notes were left, with its leading space, or nothing.
    pub(crate) fn place(&self) -> &'static str {
        match self {
            Source::Dir(_) => " on the page",
            Source::Channel {
                ..
            } => " on the channel",
            Source::Delegated(_) => "",
        }
    }
}

/// What a run works in.
pub struct Run {
    pub root:  PathBuf,
    /// The capture store, absolute.
    pub store: PathBuf,
    /// The work's own repository, which rows are looked for in when a document names no clone.
    pub work:  Option<String>,
    /// A launcher that takes mockspace's subcommands directly, in place of `cargo mock`.
    pub mock:  Option<PathBuf>,
}

/// What a `take` did.
#[derive(Debug, Default, Serialize)]
pub struct Taken {
    /// The capture written, or none when nothing was new.
    pub written:  Option<PathBuf>,
    pub answers:  usize,
    pub notes:    usize,
    /// Rows an answer was attached to, as `<namespace>::<id>`.
    pub attached: Vec<String>,
    /// What could not be attached, each with its reason; the capture still holds the words.
    pub left:     Vec<String>,
    /// Documents that failed their checks.
    pub skipped:  Vec<String>,
}

impl HumanRender for Taken {
    fn render_human(&self, out: &mut dyn Write) -> std::io::Result<()> {
        let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
        write!(out, "took {}", plural(self.answers, "answer"))?;
        if self.notes > 0 {
            write!(out, " {}", plural(self.notes, "note"))?;
        }
        Ok(())
    }
}

/// Writes what is new in `read` as one capture and attaches its answers.
///
/// `repo` is the clone every answer's row is in, where the caller knows it; else each document's
/// `where` and the work's repository are tried.
pub fn take_documents(
    run: &Run,
    read: Read,
    source: &Source<'_>,
    repo: Option<&Path>,
) -> Result<Taken> {
    let mut taken = Taken {
        skipped: read.skipped,
        ..Taken::default()
    };
    let recorded = render::recorded(&run.store)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut fresh: Vec<docs::Document> = read
        .taken
        .into_iter()
        .filter(|d| !recorded.contains(&d.key) && seen.insert(d.key.clone()))
        .collect();
    if fresh.is_empty() {
        return Ok(taken);
    }
    fresh.sort_by(|a, b| a.at.cmp(&b.at));
    let latest = fresh
        .iter()
        .map(|d| d.at.as_str())
        .max()
        .unwrap_or_default()
        .to_string();
    let title = render::title(&fresh, source);
    let name = render::name(&run.store, &latest, &title, &fresh);
    let body = render::text(&fresh, &render::when_of(&latest), source, &title);
    std::fs::create_dir_all(&run.store)
        .with_context(|| format!("making {}", run.store.display()))?;
    let file = run.store.join(&name);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
        .with_context(|| format!("writing {}, which a capture never replaces", file.display()))?;
    f.write_all(body.as_bytes())
        .with_context(|| format!("writing {}", file.display()))?;
    let shown = file
        .strip_prefix(&run.root)
        .unwrap_or(&file)
        .to_string_lossy()
        .into_owned();
    taken.answers = fresh
        .iter()
        .filter(|d| d.collection == Collection::Answers)
        .count();
    taken.notes = fresh.len() - taken.answers;
    taken.written = Some(file);
    for d in fresh.iter().filter(|d| d.collection == Collection::Answers) {
        let Some(target) = attach::target(d) else { continue };
        let Some(clone) = repo
            .map(Path::to_path_buf)
            .or_else(|| attach::repo_for(&run.root, d.text("where"), run.work.as_deref()))
        else {
            taken.left.push(format!(
                "{} is in no clone of this workspace, so its row was not touched; the capture holds the answer",
                target.spelled()
            ));
            continue;
        };
        match attach::row(run.mock.as_deref(), &clone, &target, &d.at, &shown) {
            Ok(()) => taken.attached.push(target.spelled()),
            Err(why) => taken.left.push(why),
        }
    }
    Ok(taken)
}

fn say(lines: &[String], lead: &str) {
    for l in lines {
        eprintln!("{lead}{l}");
    }
}

/// The subcommand.
pub fn run(cli: &Cli, op: &AnswersOp) -> Result<Outcome> {
    let cfg = load_config(cli)?;
    let ws = crate::cmd::org::load(&config_path(cli))?;
    let root = cfg.workspace.path.as_path();
    match op {
        AnswersOp::Hook {
            remove,
        } => {
            if *remove {
                hook::remove(root)?;
            } else {
                hook::place(
                    root,
                    &std::env::current_exe().context("finding this program")?,
                )?;
            }
            Ok(Outcome::Ok)
        },
        AnswersOp::Take {
            dir,
            channel,
            from,
            into,
        } => {
            let run = context(cli, root, &ws, into.as_deref())?;
            let (read, source) = match (dir, channel) {
                (Some(d), _) => (docs::read_dir(d)?, Source::Dir(d)),
                (None, Some(c)) => {
                    (
                        docs::read_channel(root, &docs::msg_program(root), c, from)?,
                        Source::Channel {
                            channel: c,
                            from,
                        },
                    )
                },
                (None, None) => bail!("name a --dir or a --channel to take from"),
            };
            let taken = take_documents(&run, read, &source, None)?;
            say(&taken.skipped, "skipped: ");
            say(&taken.left, "left: ");
            if taken.written.is_some() || cli.output == OutputFormat::Json {
                emit(&taken, cli.output)?;
            }
            Ok(Outcome::Ok)
        },
        AnswersOp::Delegate {
            target,
            field,
            value,
        } => {
            let run = context(cli, root, &ws, None)?;
            let repo = std::env::var_os("MOCKSPACE_REPO_ROOT")
                .map(PathBuf::from)
                .ok_or_else(|| {
                    anyhow!("MOCKSPACE_REPO_ROOT is not set; this runs from mockspace's hook")
                })?;
            match delegate::run(&run, &repo, target, field, value)? {
                delegate::Delegated::Whole => Ok(Outcome::Ok),
                delegate::Delegated::Declined => Ok(Outcome::Exit(3)),
            }
        },
    }
}

/// The run's roots and the store, checked against the places homma never writes.
fn context(cli: &Cli, root: &Path, ws: &homma_api::Workspace, into: Option<&Path>) -> Result<Run> {
    let store = into
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(ws.paths.captures.as_path()));
    let at = homma_api::AbsPath::new(std::path::absolute(&store).unwrap_or_else(|_| store.clone()))
        .map_err(|e| anyhow!("{e}"))?;
    let abs_root =
        homma_api::AbsPath::new(std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()))
            .map_err(|e| anyhow!("{e}"))?;
    homma_api::Denied::for_the_workspace(ws, &abs_root)?.check(&at, "the capture store")?;
    let work = homma_core::local::Local::load(&crate::cmd::local::manifest_dir(cli))
        .ok()
        .flatten()
        .map(|l| l.instance.work);
    Ok(Run {
        root: root.to_path_buf(),
        store,
        work,
        mock: std::env::var_os("HOMMA_MOCK")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from),
    })
}
