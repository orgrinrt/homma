//! A path proven to lie inside a root, by asking the filesystem.
//!
//! This exists because eight review rounds on one branch each guarded a path a
//! caller passed in, while the paths actually written to were computed
//! downstream and never guarded at all. Every round closed the route it was
//! shown a reproduction for and reported the class closed; the class moved.
//!
//! The reproduction the type answers: a workspace root whose tree carries a
//! symlink, say `.shared -> ../victim`, takes every lexically-computed path
//! under it into another repository. [`AbsPath::join`] clamps `..` and is
//! lexical by design, which is correct for what it is and is exactly what a
//! symlink defeats. Nothing lexical can see through one.
//!
//! So containment is established against the filesystem, and holding a
//! [`ContainedPath`] is the proof that it was. There is no other constructor,
//! which is the whole mechanism: a write computed from an unchecked path does
//! not type-check, rather than not being noticed.

use crate::AbsPath;
use std::fmt;
use std::path::Path;

/// A workspace root, with its resolved form remembered.
///
/// Resolved once, because every containment question compares against it and
/// resolving per question is both slower and a chance for the two to disagree.
#[derive(Debug, Clone)]
pub struct Root {
    /// As the caller wrote it, for error messages a human can act on.
    as_written: AbsPath,
    /// Symlinks and `..` resolved, which is what containment compares against.
    resolved: AbsPath,
}

impl Root {
    /// Resolve a root so paths can be proven against it.
    ///
    /// A root that does not exist yet resolves to itself, which is correct:
    /// nothing can be inside it via a symlink that is not there.
    pub fn new(root: &AbsPath) -> std::io::Result<Self> {
        Ok(Self {
            as_written: root.clone(),
            resolved: root.resolved()?,
        })
    }

    pub fn as_abs(&self) -> &AbsPath {
        &self.as_written
    }

    /// Prove that `path` resolves inside this root.
    ///
    /// The path need not exist. Its longest existing prefix is resolved and the
    /// remainder is taken as written, which is what makes the check usable on a
    /// path that is about to be created, and is enough: a symlink that would
    /// redirect the write has to exist for the write to follow it.
    pub fn contain(&self, path: &AbsPath) -> Result<ContainedPath, Escapes> {
        let resolved = path.resolved().map_err(|e| Escapes {
            path: path.clone(),
            root: self.as_written.clone(),
            why: Why::Unresolvable(e.to_string()),
        })?;
        if resolved.as_path().starts_with(self.resolved.as_path()) {
            Ok(ContainedPath(path.clone()))
        } else {
            Err(Escapes {
                path: path.clone(),
                root: self.as_written.clone(),
                why: Why::Outside {
                    resolved_to: resolved,
                },
            })
        }
    }

    /// Create a directory and everything above it, then prove it is still
    /// inside.
    ///
    /// Proving and then creating leaves a window: between the two, a symlink can
    /// be planted on the path. Closing it entirely means creating one component
    /// at a time and checking each, which is a larger change than this round
    /// carries. Re-proving afterwards turns a silent escape into a loud one,
    /// which is the honest half rather than the whole of it. Stated here so the
    /// residue is visible rather than implied by its absence.
    pub fn create_dir_all(&self, path: &ContainedPath) -> std::io::Result<()> {
        std::fs::create_dir_all(path)?;
        self.contain(path.as_abs())
            .map(|_| ())
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

/// A path established to resolve inside a [`Root`].
///
/// Constructed only by [`Root::contain`]. That is deliberate and is the point of
/// the type: a function taking one cannot be handed a path nobody checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedPath(AbsPath);

impl ContainedPath {
    pub fn as_abs(&self) -> &AbsPath {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn into_abs(self) -> AbsPath {
        self.0
    }
}

impl std::ops::Deref for ContainedPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.as_path()
    }
}

impl AsRef<Path> for ContainedPath {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl fmt::Display for ContainedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A path that does not resolve inside the root it was checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escapes {
    pub path: AbsPath,
    pub root: AbsPath,
    why: Why,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Why {
    Outside { resolved_to: AbsPath },
    Unresolvable(String),
}

impl fmt::Display for Escapes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.why {
            Why::Outside { resolved_to } => write!(
                f,
                "{} resolves to {}, which is outside the workspace root {}",
                self.path, resolved_to, self.root
            ),
            Why::Unresolvable(e) => write!(
                f,
                "{} could not be resolved against the workspace root {}: {}",
                self.path, self.root, e
            ),
        }
    }
}

impl std::error::Error for Escapes {}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(p: impl Into<std::path::PathBuf>) -> AbsPath {
        AbsPath::new(p).expect("a tempdir path is absolute")
    }

    #[test]
    fn a_path_under_the_root_is_proven() {
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(&abs(d.path())).unwrap();
        let inside = root.as_abs().join("hands").join("paja");
        assert!(root.contain(&inside).is_ok());
    }

    #[test]
    fn a_path_beside_the_root_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(d.path().join("sibling")).unwrap();
        let root = Root::new(&abs(&root_dir)).unwrap();
        assert!(root.contain(&abs(d.path().join("sibling"))).is_err());
    }

    // The reproduction eight review rounds did not close. It is a test rather
    // than a note because the class relocated seven times, and prose did not
    // stop it once.
    #[test]
    fn a_symlink_inside_the_root_cannot_carry_a_write_out_of_it() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        let victim = d.path().join("victim");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&victim).unwrap();
        // Exactly the shape homma itself commits: a relative link inside the
        // tree, which a clone is expected to carry.
        std::os::unix::fs::symlink("../victim", root_dir.join("shared")).unwrap();

        let root = Root::new(&abs(&root_dir)).unwrap();
        let through_the_link = root.as_abs().join("shared").join("hands").join("paja");

        let err = root
            .contain(&through_the_link)
            .expect_err("a symlink out of the root is the whole reproduction");
        assert!(
            err.to_string().contains("outside the workspace root"),
            "the message has to say what happened: {err}"
        );
    }

    // The ninth review's reproduction, and the shape the test above does not
    // reach. A link whose target does not exist is not an absent path: every
    // std write API opens with `O_CREAT` and follows it, so the write lands at
    // the target. `Path::exists()` answers `false` here, which is what made the
    // old resolution take the path as written.
    #[test]
    fn a_dangling_symlink_is_resolved_rather_than_taken_as_written() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        let victim = d.path().join("victim");
        std::fs::create_dir_all(root_dir.join("agents")).unwrap();
        std::fs::create_dir_all(&victim).unwrap();
        // The target does not exist. The link does.
        std::os::unix::fs::symlink("../../victim/r.md", root_dir.join("agents").join("r.md"))
            .unwrap();

        let root = Root::new(&abs(&root_dir)).unwrap();
        let through = root.as_abs().join("agents").join("r.md");

        let err = root
            .contain(&through)
            .expect_err("a dangling link still redirects the write that follows it");
        assert!(
            err.to_string().contains("outside the workspace root"),
            "the message has to say what happened: {err}"
        );
    }

    #[test]
    fn a_dangling_symlink_with_an_absolute_target_is_resolved_too() {
        // The worse half: an absolute target reaches anywhere, including the
        // paths the deny list names.
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        let elsewhere = d.path().join("elsewhere");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("gone.md"), root_dir.join("r.md")).unwrap();

        let root = Root::new(&abs(&root_dir)).unwrap();
        assert!(root.contain(&root.as_abs().join("r.md")).is_err());
    }

    #[test]
    fn a_symlink_cycle_is_reported_rather_than_looped() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::os::unix::fs::symlink("b", root_dir.join("a")).unwrap();
        std::os::unix::fs::symlink("a", root_dir.join("b")).unwrap();

        let root = Root::new(&abs(&root_dir)).unwrap();
        // Either answer is acceptable except hanging, so the assertion is that
        // it returns at all. A cycle inside the root is not an escape.
        let _ = root.contain(&root.as_abs().join("a"));
    }

    #[test]
    fn a_root_that_does_not_exist_yet_still_contains_its_own_children() {
        let d = tempfile::tempdir().unwrap();
        let unborn = abs(d.path().join("not-yet"));
        let root = Root::new(&unborn).unwrap();
        assert!(root.contain(&unborn.join("hands")).is_ok());
    }

    #[test]
    fn creating_a_directory_proves_it_again_afterwards() {
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(&abs(d.path())).unwrap();
        let target = root.contain(&root.as_abs().join("a").join("b")).unwrap();
        root.create_dir_all(&target).unwrap();
        assert!(target.exists());
    }

    #[test]
    fn the_root_itself_is_contained_in_itself() {
        // Otherwise the root cannot be prepared, and a boundary that excludes
        // its own edge tends to be discovered by a caller rather than a test.
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(&abs(d.path())).unwrap();
        assert!(root.contain(root.as_abs()).is_ok());
    }
}
