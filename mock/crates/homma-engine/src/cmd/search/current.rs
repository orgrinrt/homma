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
                let read = if *commits > 0 {
                    format!(", {} read", counted(*commits, "commit"))
                } else {
                    String::new()
                };
                format!(
                    "brought current: {added} added, {changed} changed, {removed} removed{read}"
                )
            },
            Self::Behind {
                files,
                commits,
                why,
            } => {
                let behind = if *commits > 0 {
                    format!(
                        "{} and {}",
                        counted(*files, "file"),
                        counted(*commits, "commit")
                    )
                } else {
                    counted(*files, "file")
                };
                format!("behind by {behind}, not brought current: {why}")
            },
            Self::Unknown {
                why,
            } => format!("how far behind is not known: {why}"),
        }
    }
}

/// `n` of `what`, in its number.
fn counted(n: usize, what: &str) -> String {
    if n == 1 { format!("1 {what}") } else { format!("{n} {what}s") }
}

/// The crate's words for a file that records no definition, with the command that writes one where
/// it is known.
fn undefined(e: &Error, build: Option<&str>) -> String {
    let writes = build
        .map(|b| format!("; {b} writes one"))
        .unwrap_or_default();
    format!("{e}{writes}")
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
            return (
                Currency::Unknown {
                    why: undefined(&e, build),
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
        Err(e @ Error::Undefined(_)) => {
            Currency::Unknown {
                why: format!("{why}; and measuring it: {}", undefined(&e, build)),
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

/// For a file the crate refused as cut by another chunker than this build reads, which way it is
/// out and what moves it: an older one is cut again by its own `build`, a newer one is read by a
/// build of homma against the crate's trunk. The question never cuts it again itself.
pub fn other_chunker(e: &Error, build: Option<&str>) -> Option<String> {
    let Error::Header {
        field: "chunker",
        stored,
        ..
    } = e
    else {
        return None;
    };
    let stored: u32 = stored.parse().ok()?;
    let reads = muisti::CHUNKER_VERSION;
    match stored.cmp(&reads) {
        std::cmp::Ordering::Less => {
            Some(match build {
                Some(b) => {
                    format!(
                        "cut again by {b}; homma reads chunker {reads}, so a build of the index \
                         crate older than that cuts it to be refused again"
                    )
                },
                None => format!("cut by an older chunker; homma reads chunker {reads}"),
            })
        },
        std::cmp::Ordering::Greater => {
            Some(format!(
                "this build of homma reads chunker {reads}; a build of homma against the index \
                 crate's trunk reads it, and the file is left as it is"
            ))
        },
        std::cmp::Ordering::Equal => None,
    }
}
