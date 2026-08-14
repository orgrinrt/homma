//! A path that cannot be relative.
//!
//! This type exists because the precondition it carries was tried twice as prose
//! and once as a runtime check, and all three failed in the same way.
//!
//! Stated in a comment, it has to be re-checked by every caller. Checked at
//! runtime in one implementation of one method, it leaves the other six
//! unchecked: three consecutive rounds each closed the route they were shown a
//! reproduction for and left the next one open, and the runtime check that was
//! finally added turned out to be unreachable in both test doubles, so nothing
//! would have failed if it were deleted.
//!
//! In a signature it is checked once, by the compiler, for every caller that
//! will ever exist. That is the whole argument, and it is the workspace's own
//! rule about harnessing the type system rather than restating a discipline.
//!
//! What a relative path costs, concretely: it resolves against whatever
//! directory the process happens to be in. A guard walking upward from one walks
//! up from there and finds the wrong answer; a clone target that is one lands in
//! whatever repository the operator was standing in, which the deny list
//! forbids outright.

use std::fmt;
use std::path::{Path, PathBuf};

/// A path known to be absolute.
///
/// There is no way to build one from a relative path without saying what it is
/// relative to, which is the point: the resolution is where the caller's
/// intention lives, and leaving it implicit is what put a workspace in the
/// wrong tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsPath(PathBuf);

impl AbsPath {
    /// An already-absolute path.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, NotAbsolute> {
        let path = path.into();
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(NotAbsolute(path))
        }
    }

    /// A path resolved against a base.
    ///
    /// An absolute `path` ignores the base, matching what `Path::join` does and
    /// what every shell does. A relative one is anchored, which is the thing a
    /// configured `workspace = "hands/rel"` always meant and never did.
    pub fn resolve(base: &AbsPath, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if path.is_absolute() {
            Self(path.to_path_buf())
        } else {
            Self(base.0.join(path))
        }
    }

    /// The process's own directory, for the one place a caller genuinely means
    /// "here".
    pub fn cwd() -> std::io::Result<Self> {
        std::env::current_dir().map(Self)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Joining keeps it absolute, so a child never has to be re-checked.
    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        Self(self.0.join(path))
    }

    /// The parent, still absolute. `None` at the filesystem root.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| Self(p.to_path_buf()))
    }

    /// Symlinks and `..` resolved, where the path exists. Left alone where it
    /// does not, since a path yet to be created is still absolute.
    pub fn canonical(&self) -> Self {
        std::fs::canonicalize(&self.0)
            .map(Self)
            .unwrap_or_else(|_| self.clone())
    }
}

impl std::ops::Deref for AbsPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for AbsPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// The one way building an [`AbsPath`] fails.
#[derive(Debug, PartialEq, Eq)]
pub struct NotAbsolute(pub PathBuf);

impl fmt::Display for NotAbsolute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is relative, and a relative path resolves against whatever \
             directory the process happens to be in",
            self.0.display()
        )
    }
}

impl std::error::Error for NotAbsolute {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_cannot_become_one_without_saying_relative_to_what() {
        assert!(AbsPath::new("sub").is_err());
        assert!(AbsPath::new("./a/b").is_err());
        assert!(AbsPath::new("../up").is_err());
        assert!(AbsPath::new("").is_err());
    }

    #[test]
    fn an_absolute_path_becomes_one() {
        assert!(AbsPath::new("/srv/content").is_ok());
    }

    #[test]
    fn resolving_anchors_a_relative_path_to_the_base() {
        // The defect: `workspace = "hands/rel"` was used raw as a clone target,
        // so it landed in whatever directory the process was in rather than
        // under the workspace root.
        let base = AbsPath::new("/srv/ws").unwrap();
        assert_eq!(
            AbsPath::resolve(&base, "hands/rel").as_path(),
            Path::new("/srv/ws/hands/rel")
        );
    }

    #[test]
    fn resolving_leaves_an_absolute_path_alone() {
        let base = AbsPath::new("/srv/ws").unwrap();
        assert_eq!(
            AbsPath::resolve(&base, "/elsewhere/h").as_path(),
            Path::new("/elsewhere/h")
        );
    }

    #[test]
    fn a_child_and_a_parent_are_both_still_absolute() {
        let p = AbsPath::new("/srv/ws").unwrap();
        assert!(p.join("a/b").is_absolute());
        assert!(p.parent().unwrap().is_absolute());
    }

    #[test]
    fn the_filesystem_root_has_no_parent() {
        assert_eq!(AbsPath::new("/").unwrap().parent(), None);
    }

    #[test]
    fn canonicalising_a_path_that_does_not_exist_leaves_it_absolute() {
        let p = AbsPath::new("/srv/does/not/exist").unwrap();
        assert!(p.canonical().is_absolute());
    }

    #[test]
    fn the_error_says_what_a_relative_path_costs() {
        // The message is what a caller sees when a registry names one, so it
        // has to explain rather than only refuse.
        let e = AbsPath::new("hands/rel").unwrap_err();
        assert!(e.to_string().contains("hands/rel"));
        assert!(e.to_string().contains("process"));
    }
}
