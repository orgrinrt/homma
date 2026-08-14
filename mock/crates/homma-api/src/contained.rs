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
//! which is the mechanism.
//!
//! **What that buys is narrower than the obvious sentence, which was compiled
//! and falsified.** It does not make an unchecked write fail to type-check:
//! `std::fs` takes anything that is `AsRef<Path>`, and every path type here is,
//! so a new function can always take a bare one. What holds is that a function
//! **declaring** a `ContainedPath` parameter cannot be called without a proof,
//! which makes the check a thing somebody chose to skip rather than a thing
//! nobody noticed.

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
    /// The absolute places nothing may be written, whatever the root is.
    ///
    /// **Held here rather than checked by the caller**, because a caller that
    /// has to remember a second check is a caller that forgets it. Checking the
    /// root and the workspace was not enough: with the root set to a home
    /// directory, every derived path passed containment and landed in
    /// `~/.claude` anyway, since a home contains itself.
    denied: crate::Denied,
}

impl Root {
    /// Resolve a root so paths can be proven against it.
    ///
    /// A root that does not exist yet resolves to itself, which is correct:
    /// nothing can be inside it via a symlink that is not there.
    ///
    /// **A root whose own parent does not exist is refused.** `create_dir_all`
    /// creates every missing ancestor, so a missing root takes its ancestors
    /// with it, and those are above the root by definition: containment says
    /// nothing about them and cannot, since they are outside the thing doing the
    /// containing. With a root under a home directory this created directories
    /// inside `~/.claude/`, which the record forbids outright.
    ///
    /// So the rule is that homma creates the root and never the path to it.
    /// Creating the root is intended and `content_repo = "local"` relies on it;
    /// making the road to it is not, and nothing ever asked for it.
    pub fn new(root: &AbsPath, denied: crate::Denied) -> std::io::Result<Self> {
        if let Some(parent) = root.parent() {
            if !parent.as_path().exists() {
                return Err(std::io::Error::other(format!(
                    "{parent} does not exist, so the workspace root {root} cannot be \
                     created without creating the path to it. homma creates the root \
                     and never its ancestors; make {parent} first."
                )));
            }
        }
        let me = Self {
            as_written: root.clone(),
            resolved: root.resolved()?,
            denied,
        };
        // The root itself, so a root that *is* a denied place is refused here
        // rather than at whichever derived path happened to be built first.
        me.denied
            .check(root, "workspace root")
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(me)
    }

    pub fn as_abs(&self) -> &AbsPath {
        &self.as_written
    }

    /// Prove that `path` resolves inside this root.
    ///
    /// The path need not exist. Every symlink on it is followed by
    /// [`AbsPath::resolved`], including one whose target does not exist, and
    /// what remains after the last real component is taken as written.
    ///
    /// **The previous version of this sentence was the type's whole safety
    /// argument and it was false.** It said a symlink had to exist for a write
    /// to follow it, which is true of the symlink and not of its target: a
    /// dangling link is a link, `O_CREAT` follows it, and the write lands
    /// outside. The argument now rests on the walk in `resolved` rather than on
    /// a claim about what `exists()` means, and the cases are pinned by tests
    /// below rather than by this paragraph.
    pub fn contain(&self, path: &AbsPath) -> Result<ContainedPath, Escapes> {
        let resolved = path.resolved().map_err(|e| Escapes {
            path: path.clone(),
            root: self.as_written.clone(),
            why: Why::Unresolvable(e.to_string()),
        })?;
        if resolved.as_path().starts_with(self.resolved.as_path()) {
            // Inside the root is necessary and not sufficient. A root that is a
            // home contains `~/.claude` perfectly, and the record denies it
            // absolutely rather than relative to anything.
            self.denied.check(path, "path").map_err(|e| Escapes {
                path: path.clone(),
                root: self.as_written.clone(),
                why: Why::Unresolvable(e.to_string()),
            })?;
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

    /// Descend from a path already proven, and prove the result.
    ///
    /// The ordinary way to derive a path. It exists because every derivation was
    /// going through [`ContainedPath::as_abs`], which hands back an [`AbsPath`]
    /// carrying `join`, `parent` and a full `Deref`, so the property that a
    /// derived path gets re-proven was a convention every caller had to keep
    /// rather than something the types enforced. Callers that keep a convention
    /// correctly four times out of four are still one careless call from the
    /// eleventh review.
    pub fn contain_under(
        &self,
        base: &ContainedPath,
        tail: impl AsRef<Path>,
    ) -> Result<ContainedPath, Escapes> {
        self.contain(&base.0.join(tail))
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
///
/// **It deliberately does not implement `Deref<Target = Path>`.** It did, and
/// that handed every consumer `Path::join` and `Path::parent` on a proven path.
/// Neither preserves the proof, and `join` with an absolute argument discards
/// the receiver outright, so the guarantee was voidable by accident by anybody
/// who did not know to avoid it.
///
/// **That is a narrower property than it sounds and the difference matters.**
/// [`ContainedPath::as_abs`] is a declared unwrap door, so a caller that wants
/// `join` can still reach it in two steps. What the missing `Deref` buys is that
/// the two steps are visible: deriving a path is a thing somebody wrote down
/// rather than a thing that happened. Use [`Root::contain_under`] and the door
/// is not needed at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedPath(AbsPath);

impl ContainedPath {
    /// The proven path as an [`AbsPath`], **which is an unwrap door and is
    /// declared as one**.
    ///
    /// An `AbsPath` carries `join`, `parent` and `Deref<Target = Path>`, none of
    /// which preserves the proof, so anything derived from the result has to go
    /// back through [`Root::contain`] before it is written to. That is a
    /// convention rather than a type property, which is why
    /// [`Root::contain_under`] exists and why this should be rare.
    ///
    /// `what-you-can-observe-is-what-you-guaranteed.md` permits a door of this
    /// shape when it is documented as one. It is the same reasoning that lets a
    /// `Transparent` newtype hand out its inner value: the perimeter stays
    /// closed because the opening is named, not because nothing opens.
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

    // The twelfth review's reproduction. `create_dir_all` creates every missing
    // ancestor, so a missing root takes its own ancestors with it, and those are
    // above the root by definition. With the root under a home directory this
    // created directories inside `~/.claude/`, which the record forbids.
    #[test]
    fn a_root_whose_parent_does_not_exist_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let deep = abs(d.path().join("a").join("b").join("newroot"));
        let err = Root::new(
            &deep,
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .expect_err("homma creates the root, never the path to it");
        assert!(
            err.to_string().contains("does not exist"),
            "the message has to say what is missing: {err}"
        );
        assert!(
            !d.path().join("a").exists(),
            "and refusing must not have created it on the way"
        );
    }

    #[test]
    fn a_root_that_does_not_exist_but_whose_parent_does_is_allowed() {
        // Creating the root itself is intended: `content_repo = "local"` relies
        // on it. Only the path to it is refused.
        let d = tempfile::tempdir().unwrap();
        assert!(Root::new(
            &abs(d.path().join("newroot")),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap())
        )
        .is_ok());
    }

    // The guard here was live and pinned by nothing: replacing the tail of
    // `create_dir_all` with `Ok(())` left the whole suite green, in a file whose
    // header sells that guard as turning a silent escape into a loud one.
    #[test]
    fn creating_a_directory_reports_a_link_planted_after_the_proof() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        let outside = d.path().join("outside");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        let target = root.contain(&root.as_abs().join("planted")).unwrap();

        // Between the proof and the creation, which is the window the type
        // documents as open rather than closed.
        std::os::unix::fs::symlink(&outside, root_dir.join("planted")).unwrap();

        assert!(
            root.create_dir_all(&target).is_err(),
            "a link planted after the proof must be reported, not followed silently"
        );
    }

    #[test]
    fn a_path_under_the_root_is_proven() {
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(
            &abs(d.path()),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        let inside = root.as_abs().join("hands").join("paja");
        assert!(root.contain(&inside).is_ok());
    }

    #[test]
    fn a_path_beside_the_root_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(d.path().join("sibling")).unwrap();
        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
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

        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
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

        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
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

        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        assert!(root.contain(&root.as_abs().join("r.md")).is_err());
    }

    #[test]
    fn a_symlink_cycle_is_reported_rather_than_looped() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::os::unix::fs::symlink("b", root_dir.join("a")).unwrap();
        std::os::unix::fs::symlink("a", root_dir.join("b")).unwrap();

        let root = Root::new(
            &abs(&root_dir),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        // Either answer is acceptable except hanging, so the assertion is that
        // it returns at all. A cycle inside the root is not an escape.
        let _ = root.contain(&root.as_abs().join("a"));
    }

    // `resolved` is a reimplementation of something the kernel already does, so
    // it is measured against the kernel rather than against what its author
    // expected. `canonicalize` needs the path to exist, so this covers only the
    // half both can answer, which is exactly the half where a disagreement is
    // checkable.
    //
    // The half `canonicalize` cannot answer, a path that does not exist yet, is
    // the reason `resolved` exists at all and is covered by the tests above.
    #[test]
    fn resolution_agrees_with_the_kernel_wherever_the_kernel_can_answer() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path();
        std::fs::create_dir_all(base.join("real/deep/nest")).unwrap();
        std::fs::create_dir_all(base.join("other")).unwrap();
        std::fs::write(base.join("real/deep/nest/file"), "x").unwrap();

        // relative target, absolute target, a chain, a target carrying `..`,
        // a link used as an interior component, and a link to a link to a file.
        std::os::unix::fs::symlink("real/deep", base.join("rel")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("absl")).unwrap();
        std::os::unix::fs::symlink("rel", base.join("chain")).unwrap();
        std::os::unix::fs::symlink("../other", base.join("real/up")).unwrap();
        std::os::unix::fs::symlink("real/deep/nest/file", base.join("tofile")).unwrap();
        std::os::unix::fs::symlink("tofile", base.join("totofile")).unwrap();

        let cases = [
            "rel",
            "rel/nest",
            "rel/nest/file",
            "absl",
            "absl/deep/nest",
            "chain",
            "chain/nest",
            "real/up",
            "tofile",
            "totofile",
            "real/deep/nest",
        ];

        for case in cases {
            let p = abs(base.join(case));
            let kernel = std::fs::canonicalize(base.join(case)).unwrap_or_else(|e| {
                panic!("the case must exist for this to mean anything: {case}: {e}")
            });
            let ours = p.resolved().unwrap();
            assert_eq!(
                ours.as_path(),
                kernel,
                "resolution disagrees with the kernel on {case}"
            );
        }
    }

    #[test]
    fn a_root_that_does_not_exist_yet_still_contains_its_own_children() {
        let d = tempfile::tempdir().unwrap();
        let unborn = abs(d.path().join("not-yet"));
        let root = Root::new(
            &unborn,
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        assert!(root.contain(&unborn.join("hands")).is_ok());
    }

    #[test]
    fn creating_a_directory_proves_it_again_afterwards() {
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(
            &abs(d.path()),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        let target = root.contain(&root.as_abs().join("a").join("b")).unwrap();
        root.create_dir_all(&target).unwrap();
        assert!(target.as_path().exists());
    }

    #[test]
    fn the_root_itself_is_contained_in_itself() {
        // Otherwise the root cannot be prepared, and a boundary that excludes
        // its own edge tends to be discovered by a caller rather than a test.
        let d = tempfile::tempdir().unwrap();
        let root = Root::new(
            &abs(d.path()),
            crate::Denied::under_home(&AbsPath::new("/nonexistent-home").unwrap()),
        )
        .unwrap();
        assert!(root.contain(root.as_abs()).is_ok());
    }
}
