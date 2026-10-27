//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The ways git can reach, or fail to reach, one entrypoint, and how each is
//! said. How it is read off the repository is `install::reach`.

use std::fmt;
use std::path::PathBuf;

/// How git reaches one entrypoint, read off `core.hooksPath` and, where one
/// is set, the file git will run under it for that event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// No hooks path: git reads the directory itself.
    Direct,
    /// No hooks path, and no entrypoint of homma's in git's own directory yet,
    /// so git runs nothing for this event until `homma hook install` writes it.
    NotInstalled(PathBuf),
    /// The hook under the path is mockspace's, whose gate runs the
    /// repository's own hook after its own checks.
    Chained(PathBuf),
    /// Nothing under the path for this event, so git runs nothing for it.
    Missing(PathBuf),
    /// The path is git-shook's dispatcher directory and homma's entry for this
    /// event is registered there. git-shook chains to nothing, so homma is at
    /// the event as a registered entry beside the others and not through the
    /// entrypoint; the path is the entry file.
    Shook(PathBuf),
    /// The path is git-shook's dispatcher directory and nothing of homma's is
    /// registered for this event, so it runs nothing here until
    /// `homma hook install` registers the manifest it generates.
    ShookUnregistered(PathBuf),
    /// A hook under the path that is some other tool's, and nothing runs the
    /// entrypoint until that tool chains to it.
    Foreign(PathBuf),
    /// No hooks path, and the file in git's own directory is not the text
    /// homma writes, so git runs that file in place of the entrypoint.
    Other(PathBuf),
}

impl Reach {
    pub fn reached(&self) -> bool {
        matches!(self, Reach::Direct | Reach::Chained(_) | Reach::Shook(_))
    }
}

impl fmt::Display for Reach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reach::Direct => f.write_str("git reads the hooks directory directly"),
            Reach::Chained(p) => {
                write!(
                    f,
                    "chains through {}, mockspace's, after its own checks",
                    p.display()
                )
            },
            Reach::NotInstalled(p) => {
                write!(
                    f,
                    "nothing at {}; `homma hook install` writes the entrypoint",
                    p.display()
                )
            },
            Reach::Missing(p) => {
                write!(
                    f,
                    "nothing at {}, so git will not run this until whatever owns core.hooksPath \
                     chains to the repository's own hooks",
                    p.display()
                )
            },
            Reach::Foreign(p) => {
                write!(
                    f,
                    "{} is not mockspace's; git will not run this until that tool chains to the \
                     repository's own hooks",
                    p.display()
                )
            },
            Reach::Other(p) => {
                write!(
                    f,
                    "{} is somebody else's hook, which git runs in place of the entrypoint; move \
                     what it does into the hooks table or remove it, then `homma hook install`",
                    p.display()
                )
            },
            Reach::Shook(p) => {
                write!(f, "registered with git-shook, as {}", p.display())
            },
            Reach::ShookUnregistered(p) => {
                write!(
                    f,
                    "{} is git-shook's, which runs what is registered with it and chains to \
                     nothing; `homma hook install` writes the manifest and registers it",
                    p.display()
                )
            },
        }
    }
}
