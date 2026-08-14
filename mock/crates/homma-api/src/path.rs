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

    /// This path with every symlink on it followed, whether or not the thing at
    /// the end exists.
    ///
    /// **A dangling symlink is not an absent path**, and assuming otherwise was
    /// a real hole rather than a theoretical one. The previous shape found the
    /// longest prefix satisfying `Path::exists()` and canonicalised that.
    /// `exists()` follows the link, so a link whose target is missing answers
    /// `false`, gets popped off as absent, and is re-appended **as written**;
    /// `canonicalize` then fails on it too, so the fallback returned the path
    /// unresolved. Every `std` write opens with `O_CREAT` and follows the link,
    /// so the write landed at the target while the guard was comparing the link.
    ///
    /// Measured, on a link whose target does not exist: `exists()` is `false`,
    /// `symlink_metadata()` is `Ok`, `canonicalize()` is `Err(NotFound)`.
    ///
    /// So this walks the components instead, the way the kernel does. Each
    /// prefix is examined with `symlink_metadata`, which reports a dangling link
    /// as the link it is, and `read_link` reads one whether or not its target
    /// exists. A target that is absolute restarts the walk at the root; a
    /// relative one continues from where the link sat. What does not exist and
    /// is not a link is taken as written, which is what keeps this usable on a
    /// path that is about to be created.
    ///
    /// A `..` **inside a link target** is handled, because a target may carry one
    /// and it applies to wherever the link landed.
    ///
    /// A `..` written in the path itself is not, and there is no arm for it. One
    /// existed and was unreachable: every `AbsPath` constructor normalises
    /// lexically first, so nothing can put a `ParentDir` here from `self`. It was
    /// also wrong where it could not run. Measured against the kernel,
    /// `/base/x/y/L/..` with `L -> /base/z` is `/base`, and that arm gave
    /// `/base/x/y`. Dead defence that reads as live defence is what this file
    /// deleted a branch for two rounds ago.
    pub fn resolved(&self) -> std::io::Result<Self> {
        use std::collections::VecDeque;
        use std::ffi::OsString;

        // A cycle is a real filesystem state and hanging is not an acceptable
        // answer to it. Linux uses 40; the number matters less than its
        // existence.
        const MAX_HOPS: usize = 40;

        let mut pending: VecDeque<OsString> = self
            .0
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(n) => Some(n.to_os_string()),
                // No `ParentDir` arm: `normalise` removed them before this ran.
                _ => None,
            })
            .collect();

        let mut out = PathBuf::from("/");
        let mut hops = 0usize;

        while let Some(name) = pending.pop_front() {
            if name == ".." {
                out.pop();
                continue;
            }
            if name == "." {
                continue;
            }
            let candidate = out.join(&name);
            match std::fs::symlink_metadata(&candidate) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    hops += 1;
                    if hops > MAX_HOPS {
                        // `ErrorKind::FilesystemLoop` says this exactly and is
                        // unstable, and a feature gate is not worth an error
                        // kind. The message carries what the kind would.
                        return Err(std::io::Error::other(format!(
                            "too many symbolic links resolving {}",
                            self.0.display()
                        )));
                    }
                    let target = std::fs::read_link(&candidate)?;
                    let mut front: Vec<OsString> = Vec::new();
                    for c in target.components() {
                        match c {
                            std::path::Component::RootDir => out = PathBuf::from("/"),
                            std::path::Component::Prefix(_) => {}
                            std::path::Component::CurDir => {}
                            std::path::Component::ParentDir => front.push(OsString::from("..")),
                            std::path::Component::Normal(n) => front.push(n.to_os_string()),
                        }
                    }
                    // An absolute target has already reset `out`; a relative one
                    // continues from the directory the link sat in, which is
                    // `out` as it stands.
                    for n in front.into_iter().rev() {
                        pending.push_front(n);
                    }
                }
                // Present and not a link, or absent. Either way it is taken as
                // written: an absent component cannot redirect anything, and a
                // real directory is where it says it is.
                _ => out.push(name),
            }
        }
        Ok(Self(out))
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
                // filesystem does with `/..`, and `PathBuf::pop` already does
                // exactly that: it returns false and changes nothing when there
                // is no parent. There was an `if out.parent().is_some()` here
                // and it could not be false in any way that mattered, which
                // makes it a condition that reads as a guard and is not one.
                out.pop();
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
        // There was an `is_absolute()` early return here and it was
        // unreachable: an absolute path leads with a root or prefix component,
        // which the loop below already refuses. Deleting it failed nothing,
        // which is the test that says it was doing nothing, and dead defence
        // that reads as live defence is what this file has spent eight review
        // rounds on.
        //
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
    fn descending_cannot_leave_the_path_it_descends_from() {
        // `join` inherited `PathBuf::join`, where an absolute argument discards
        // the receiver and `..` climbs past it, which is how a configured
        // `hands = "../victim/stolen"` wrote into another repository's tree.
        //
        // **This needs its own test and a measurement is why.** `RelPath`
        // refuses such a value at the configuration boundary, so this is
        // defence in depth, and defence nothing pins is defence somebody
        // deletes.
        //
        // Measured on the whole workspace with `--no-fail-fast`, because
        // stopping at the first failing binary is what produced the wrong count
        // the first three times this was reported. Loosening `join` back to
        // `PathBuf::join` fails exactly two tests: this one, and
        // `a_path_never_carries_a_parent_component`, which asserts
        // `base.join("a/../b")`. An earlier version of this comment claimed it
        // failed nothing else, which was wrong when written.
        let base = AbsPath::new("/srv/ws").unwrap();
        assert_eq!(
            base.join("../escape").as_path(),
            Path::new("/srv/ws/escape")
        );
        assert_eq!(
            base.join("../../../etc").as_path(),
            Path::new("/srv/ws/etc")
        );
        assert_eq!(
            base.join("/etc/passwd").as_path(),
            Path::new("/srv/ws/etc/passwd")
        );
        assert_eq!(base.join("a/../../b").as_path(), Path::new("/srv/ws/b"));
    }

    #[test]
    fn anchoring_may_leave_because_that_is_what_it_is_for() {
        // The other half of the split, asserted so nobody closes it by
        // symmetry. A workspace legitimately lives outside the content
        // repository, so climbing here is correct; what guards it is the
        // containment check against the filesystem, not the arithmetic.
        let base = AbsPath::new("/srv/ws").unwrap();
        assert_eq!(
            AbsPath::resolve(&base, "../out/paja").as_path(),
            Path::new("/srv/out/paja")
        );
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
