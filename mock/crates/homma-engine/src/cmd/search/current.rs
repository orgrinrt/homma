//! An index brought current before it is asked, through the index crate's own refresh under the
//! definition its file records; and where that cannot be, how far behind it is and why.
//!
//! Nothing here walks, lists or measures: the refresh and the measure are the index crate's, read
//! off the file itself. What is homma's is which of the two is tried and how the outcome is said.

use muisti::layer::Index;
use muisti::{Embedder, Error};
use serde::Serialize;

/// How current an index was made for the question, or how far from current it was asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Currency {
    /// Brought current: what the refresh moved, nothing at all where it was current already.
    Refreshed {
        added:   usize,
        changed: usize,
        removed: usize,
        /// Commits of a repository's history read, new to the index.
        commits: usize,
    },
    /// Not brought current, and measured.
    Behind {
        /// Files new, moved or gone since the index read them.
        files:   usize,
        /// Commits the index does not hold, where it keeps a history.
        commits: usize,
        /// Why it was not brought current.
        why:     String,
    },
    /// Not brought current, and not measurable.
    Unknown {
        /// Why neither could be done.
        why: String,
    },
}

impl Currency {
    /// The outcome as the index's line says it.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::Refreshed {
                added: 0,
                changed: 0,
                removed: 0,
                commits: 0,
            } => "current".to_string(),
            Self::Refreshed {
                added,
                changed,
                removed,
                commits,
            } => {
                let read =
                    if *commits > 0 { format!(", {commits} commits read") } else { String::new() };
                format!(
                    "brought current: {added} added, {changed} changed, {removed} removed{read}"
                )
            },
            Self::Behind {
                files,
                commits,
                why,
            } => {
                let s = if *files == 1 { "" } else { "s" };
                let behind = if *commits > 0 {
                    format!("{files} file{s} and {commits} commits")
                } else {
                    format!("{files} file{s}")
                };
                format!("behind by {behind}, not brought current: {why}")
            },
            Self::Unknown {
                why,
            } => format!("how far behind is not known: {why}"),
        }
    }
}

/// Bring `index` current with `embedder`; where that cannot be, measure how far behind it is and
/// say why. The index stays open and readable either way, so it is asked as it stands after. A
/// file that records no definition is named with `build`, the command that writes one, where known.
/// Beside the outcome, what the refresh could not read, one sentence each.
pub fn bring<E: Embedder>(
    index: &mut Index,
    embedder: &mut E,
    build: Option<&str>,
) -> (Currency, Vec<String>) {
    let why = match index.refresh(embedder) {
        Ok(done) => {
            return (
                Currency::Refreshed {
                    added:   done.added,
                    changed: done.changed,
                    removed: done.removed,
                    commits: done.commits,
                },
                done.caveats,
            );
        },
        Err(Error::Header {
            field: "model",
            stored,
            ..
        }) => {
            format!(
                "its vectors are `{stored}` and this search brings `{}`",
                embedder.model()
            )
        },
        Err(e @ Error::Undefined(_)) => {
            let writes = build
                .map(|b| format!("; {b} writes one"))
                .unwrap_or_default();
            return (
                Currency::Unknown {
                    why: format!("{e}{writes}"),
                },
                Vec::new(),
            );
        },
        Err(e) => e.to_string(),
    };
    let measured = match index.behind() {
        Ok(b) => {
            let mut why = why;
            for more in [b.definition.as_deref(), b.history.as_deref()]
                .into_iter()
                .flatten()
            {
                why.push_str("; ");
                why.push_str(more);
            }
            Currency::Behind {
                files: b.len(),
                commits: b.commits,
                why,
            }
        },
        Err(e) => {
            Currency::Unknown {
                why: format!("{why}; and measuring it: {e}"),
            }
        },
    };
    (measured, Vec::new())
}
