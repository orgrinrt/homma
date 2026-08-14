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
            Ok(Self(normalise(&path)))
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
            Self(normalise(path))
        } else {
            Self(normalise(&base.0.join(path)))
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
        Self(normalise(&self.0.join(path)))
    }

    /// The parent, still absolute. `None` at the filesystem root.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| Self(p.to_path_buf()))
    }

    /// Symlinks and `..` resolved.
    ///
    /// **Absence is not failure and failure is not absence.** A path yet to be
    /// created is still absolute and is returned as written; a symlink loop or
    /// a permission error is a real failure and is reported, because the result
    /// is what a containment guard walks from and an unresolved path there
    /// answers a different question than the one asked.
    pub fn canonical(&self) -> std::io::Result<Self> {
        match std::fs::canonicalize(&self.0) {
            Ok(p) => Ok(Self(p)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(self.clone()),
            Err(e) => Err(e),
        }
    }
}

/// Remove `.` and resolve `..` lexically.
///
/// **An `AbsPath` never carries either**, and that is load-bearing rather than
/// cosmetic: a containment check walks parents, and `/root/../sibling` walked
/// lexically reports `/root` as its ancestor, so a sibling directory reads as
/// nested and a correct workspace is refused. Resolving on the filesystem is not
/// enough, because the path being created does not exist yet.
///
/// Lexical resolution differs from the filesystem's where a symlink is
/// involved: `/a/link/..` is `/a` here and may be elsewhere in fact. Callers
/// that need the truth call [`AbsPath::canonical`], which asks the filesystem;
/// this is what can be known about a path that has yet to exist.
fn normalise(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Popping past the root leaves the root, which is what every
                // filesystem does with `/..`.
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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
    fn a_path_never_carries_a_parent_component() {
        // The containment check walks parents lexically, so `/root/../out`
        // reported `/root` as its ancestor and a sibling read as nested.
        assert_eq!(
            AbsPath::new("/a/b/../c").unwrap().as_path(),
            Path::new("/a/c")
        );
        let base = AbsPath::new("/srv/ws").unwrap();
        assert_eq!(
            AbsPath::resolve(&base, "../out/rel").as_path(),
            Path::new("/srv/out/rel")
        );
        assert_eq!(
            base.join("a/../b").as_path(),
            Path::new("/srv/ws/b")
        );
    }

    #[test]
    fn a_current_directory_component_is_dropped() {
        assert_eq!(
            AbsPath::new("/a/./b").unwrap().as_path(),
            Path::new("/a/b")
        );
    }

    #[test]
    fn popping_past_the_root_stays_at_the_root() {
        assert_eq!(AbsPath::new("/../..").unwrap().as_path(), Path::new("/"));
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
        assert!(p.canonical().unwrap().is_absolute());
    }

    #[test]
    fn a_symlink_loop_is_reported_rather_than_silently_unresolved() {
        // Swallowing this yielded an unresolved path, and that path is what a
        // containment guard walks from, so the guard would answer a question
        // about a path nobody asked about.
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();
        assert!(AbsPath::new(a).unwrap().canonical().is_err());
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
