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
//! whatever repository the operator was standing in, which is what every guard
//! in this crate exists to stop.

use serde::{Deserialize, Serialize};
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
        // Through the same constructor as everything else. It skipped both the
        // check and the normalisation, which is harmless on every system that
        // returns a resolved absolute path and is still a hole in the perimeter
        // of the one guarantee this type carries.
        let here = std::env::current_dir()?;
        Self::new(here).map_err(|e| std::io::Error::other(e.to_string()))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Descend into this path. **The result is always underneath it.**
    ///
    /// A parent component clamps here rather than at the filesystem root, and an
    /// absolute argument is treated as relative rather than replacing the
    /// receiver, which is what `Path::join` does and what let a configured
    /// `hands = "../victim/stolen"` write into another repository's tree.
    ///
    /// This is descending, not anchoring. Anchoring a path the caller is
    /// entitled to put anywhere is [`AbsPath::resolve`], which may leave and is
    /// guarded against the filesystem instead.
    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        use std::path::Component;
        let mut out = self.0.clone();
        for c in path.as_ref().components() {
            match c {
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
                Component::ParentDir => {
                    // Clamped at the receiver. Popping past it is what leaving
                    // means, and nothing descending has business doing it.
                    if out != self.0 {
                        out.pop();
                    }
                }
                Component::Normal(n) => out.push(n),
            }
        }
        Self(out)
    }

    /// The parent, still absolute. `None` at the filesystem root.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| Self(p.to_path_buf()))
    }

    /// This path with its **existing prefix** resolved and the rest left as
    /// written.
    ///
    /// `canonical` fails on a path that does not exist, and a path being created
    /// never does, so comparing one against a resolved path never matches: on a
    /// system where `/var` is a symlink to `/private/var`, a canonical root and
    /// an unresolved target are never in a containment relation even when one
    /// plainly contains the other. That mismatch has now produced the same
    /// defect twice, so the resolution lives here rather than in whichever
    /// caller last needed it.
    pub fn resolved(&self) -> std::io::Result<Self> {
        let mut existing = self.clone();
        let mut rest: Vec<std::ffi::OsString> = Vec::new();
        loop {
            if existing.exists() {
                break;
            }
            match (
                existing.file_name().map(|n| n.to_os_string()),
                existing.parent(),
            ) {
                (Some(name), Some(parent)) => {
                    rest.push(name);
                    existing = parent;
                }
                // Nothing on the way up exists, which an absolute path with a
                // root cannot manage, but is not worth an unwrap.
                _ => return Ok(self.clone()),
            }
        }
        let mut out = existing.canonical()?;
        for name in rest.into_iter().rev() {
            out = out.join(name);
        }
        Ok(out)
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

/// A relative path that cannot escape whatever it is joined to.
///
/// Configuration supplies these, and configuration is the surface an escape
/// arrives on: `hands = "../victim/stolen"` and `hands = "/etc"` both reached
/// another tree before this existed. Refused **when the configuration is
/// parsed**, so the failure is reported where somebody can act on it rather than
/// where a file lands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RelPath(PathBuf);

impl RelPath {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, NotContained> {
        let path = path.into();
        if path.is_absolute() {
            return Err(NotContained(path));
        }
        // Not merely "contains no `..`": `a/../b` is fine and stays inside.
        // What is refused is a path whose normalisation leaves.
        let mut depth: i32 = 0;
        for c in path.components() {
            match c {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(NotContained(path));
                    }
                }
                std::path::Component::Normal(_) => depth += 1,
                _ => return Err(NotContained(path)),
            }
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for RelPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        RelPath::new(raw).map_err(serde::de::Error::custom)
    }
}

/// A configured path that would leave the tree it is joined to.
#[derive(Debug, PartialEq, Eq)]
pub struct NotContained(pub PathBuf);

impl fmt::Display for NotContained {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} leaves the workspace it would be joined to. A configured path \
             is relative and stays inside; an absolute one, or one climbing past \
             the root with `..`, addresses a tree that is not ours",
            self.0.display()
        )
    }
}

impl std::error::Error for NotContained {}

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
        assert_eq!(base.join("a/../b").as_path(), Path::new("/srv/ws/b"));
    }

    #[test]
    fn a_current_directory_component_is_dropped() {
        assert_eq!(AbsPath::new("/a/./b").unwrap().as_path(), Path::new("/a/b"));
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
