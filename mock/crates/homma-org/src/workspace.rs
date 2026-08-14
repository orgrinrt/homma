//! Turning a registry entry into a directory on disk.
//!
//! Creating a workspace is: clone the content repository, set the git identity
//! in that clone's own configuration, link the memory directory into the place
//! the agent harness expects, and generate the definition.
//!
//! The memory link is the non-obvious part. The harness writes agent memory to a
//! path it chooses, `.claude/agent-memory/<handle>/`, and the layout wants that
//! memory beside everything else belonging to the same identity. A relative
//! symlink from the harness's path to the layout's satisfies both: writes
//! through it land in the layout's directory, and version control carries it as
//! a link rather than a copy, so it survives cloning to any machine.

use homma_api::{AbsPath, ContainedPath, Escapes, Identity, Paths, Root};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Where things sit inside one workspace.
///
/// **Every accessor that hands out a path hands out a proven one.** What that
/// buys is narrower than it sounds and worth stating exactly, because a broader
/// version of this sentence was compiled and falsified: `std::fs` accepts
/// anything that is `AsRef<Path>`, so nothing stops a future function here
/// taking a bare path. What holds is that `Root::create_dir_all` and
/// `link_memory` take a `ContainedPath`, so those two cannot be called without
/// one, and that `prepare` and `write_definitions` use only these accessors.
///
/// The accessors were lexical before, built with [`AbsPath::join`], which clamps
/// `..` and cannot see through a symlink. A root whose tree carried
/// `.shared -> ../victim` took every path under it into another repository, and
/// homma commits symlinks itself, so a clone is expected to carry them.
pub struct Layout<'a> {
    root: Root,
    paths: &'a Paths,
}

impl<'a> Layout<'a> {
    pub fn new(root: &AbsPath, paths: &'a Paths) -> io::Result<Self> {
        Ok(Self {
            root: Root::new(root)?,
            paths,
        })
    }

    /// Everything belonging to one identity lives under one directory.
    pub fn home(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        let base = if id.role.has_workspace() {
            &self.paths.hands
        } else {
            &self.paths.experts
        };
        self.contain(self.root.as_abs().join(base).join(&id.handle))
    }

    pub fn memory(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.under_home(id, "memory")
    }

    pub fn notes(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.under_home(id, "notes")
    }

    pub fn character(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.under_home(id, "character.md")
    }

    /// Where the harness expects to find this identity's memory.
    pub fn harness_memory(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.contain(
            self.root
                .as_abs()
                .join(".claude")
                .join("agent-memory")
                .join(&id.handle),
        )
    }

    pub fn definition(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.contain(
            self.root
                .as_abs()
                .join(&self.paths.agents)
                .join(format!("{}.md", id.handle)),
        )
    }

    /// The definition a twin runs under.
    ///
    /// A separate file because the restriction that a twin may not write memory
    /// has to be structural: a definition carrying the memory key grants the
    /// write path whatever its prose says, so the twin's simply does not carry
    /// it.
    pub fn twin_definition(&self, id: &Identity) -> Result<ContainedPath, Escapes> {
        self.contain(
            self.root
                .as_abs()
                .join(&self.paths.agents)
                .join(format!("{}-twin.md", id.handle)),
        )
    }

    /// The root as the thing that mints proofs, for a caller that has to create
    /// a directory and wants the re-check that comes with it.
    pub fn contained_root(&self) -> &Root {
        &self.root
    }

    fn under_home(&self, id: &Identity, tail: &str) -> Result<ContainedPath, Escapes> {
        // Through `home` rather than beside it, so a home that escapes is
        // caught once instead of three times differently, and through
        // `contain_under` rather than the unwrap door, so deriving a path is one
        // call to the thing that proves them.
        self.root.contain_under(&self.home(id)?, tail)
    }

    fn contain(&self, path: AbsPath) -> Result<ContainedPath, Escapes> {
        self.root.contain(&path)
    }
}

/// What was done to stand an identity up, so a caller can report it rather than
/// guess.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepared {
    pub home: ContainedPath,
    pub memory: ContainedPath,
    pub notes: ContainedPath,
    pub harness_link: ContainedPath,
    pub definition: ContainedPath,
    pub twin_definition: ContainedPath,
}

/// Create the directories an identity needs and link its memory where the
/// harness will look for it.
///
/// Idempotent: running it against a workspace that already has them changes
/// nothing and returns the same answer.
pub fn prepare(layout: &Layout<'_>, id: &Identity) -> io::Result<Prepared> {
    let root = layout.contained_root();
    let home = layout.home(id).map_err(escaped)?;
    let memory = layout.memory(id).map_err(escaped)?;
    let notes = layout.notes(id).map_err(escaped)?;
    if id.role.has_memory() {
        root.create_dir_all(&memory)?;
    }
    if id.role.has_workspace() {
        root.create_dir_all(&notes)?;
    }

    let link = layout.harness_memory(id).map_err(escaped)?;
    if id.role.has_memory() {
        if let Some(parent) = link.as_abs().parent() {
            // Proven separately, because a parent derived by arithmetic from a
            // proven path is not itself proven: `..` off a symlinked child
            // lands somewhere the child's proof says nothing about.
            let parent = root.contain(&parent).map_err(escaped)?;
            root.create_dir_all(&parent)?;
        }
        link_memory(&link, &memory)?;
    }

    let definition = layout.definition(id).map_err(escaped)?;
    if let Some(parent) = definition.as_abs().parent() {
        let parent = root.contain(&parent).map_err(escaped)?;
        root.create_dir_all(&parent)?;
    }

    Ok(Prepared {
        home,
        memory,
        notes,
        harness_link: link,
        definition,
        twin_definition: layout.twin_definition(id).map_err(escaped)?,
    })
}

/// A path that left the workspace is an io failure to the caller, and carries
/// its own explanation of where it went.
fn escaped(e: Escapes) -> io::Error {
    io::Error::other(e.to_string())
}

/// Point `link` at `target`, relatively, replacing an existing link.
///
/// Relative because an absolute target is a path on one machine, and the link is
/// committed: a clone elsewhere must resolve it.
///
/// **Both arguments are proven paths and that is not decoration.** It took a
/// `Root` too, for one round, purely to feed a containment check on the computed
/// body that could not fail. The parameter went with the check. This function
/// performs `remove_file` and `symlink`, which are two of the most consequential
/// writes in the crate, and it took two `&Path` while the module above claimed
/// an unproven write no longer compiled. It was private with one caller, so the
/// claim was true in practice and false as stated, which is the failure mode
/// this branch has been correcting since. The claim itself has since been
/// narrowed everywhere it appears: see the module header.
fn link_memory(link: &ContainedPath, target: &ContainedPath) -> io::Result<()> {
    // **The body of a symlink is a path**, in exactly the sense `Root` exists to
    // prove: the kernel follows it on every `open`. Nothing proved it, and both
    // arguments being proven was mistaken for the whole job.
    //
    // It was computed lexically from the paths *as written*, while the link is
    // created where those paths *resolve*. A link in the chain that makes the
    // real location shallower than the written one left the `..` sequence
    // climbing a level too far, and the escape was committed, because this link
    // is git mode 120000 by design.
    //
    // Both ends are resolved first, and that is what makes the result correct:
    // two paths inside the root have a relative path between them that ascends
    // only to their common ancestor, which is at or below the root.
    //
    // **Both ends being inside is not free and is not established here.** The
    // target is proven by `Layout`; the link's parent is proven by the caller
    // above, in the branch that creates it, and that check is the one doing the
    // work. A parent chain that leaves the root while the final component points
    // back in passes containment on the link itself, because resolution follows
    // the last component. `a_parent_chain_that_leaves_is_refused_even_when_the_last_link_returns`
    // is the test, and removing that check fails it and nothing else.
    //
    // A round added a `contain` call on the computed body here and called it
    // proof. It could not fail: `relative_from` expresses the target relative to
    // a resolved directory, so normalising the join recovers the target exactly,
    // and the target had been proven one frame earlier. Deleting it changed no
    // test. A guard that cannot fail is a paragraph with an `if` around it.
    let link_dir = link
        .as_abs()
        .parent()
        .ok_or_else(|| io::Error::other("the memory link has no parent directory"))?
        .resolved()?;
    let relative = relative_from(link_dir.as_path(), target.as_abs().resolved()?.as_path());

    let link = link.as_path();
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(link)? == relative {
                return Ok(());
            }
            fs::remove_file(link)?;
        }
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} exists and is not a symlink; refusing to replace it",
                    link.display()
                ),
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::os::unix::fs::symlink(&relative, link)
}

/// A path from `from` to `to`, both absolute, using `..` as needed.
fn relative_from(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let shared = from
        .iter()
        .zip(to.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out = PathBuf::new();
    for _ in shared..from.len() {
        out.push("..");
    }
    for c in &to[shared..] {
        out.push(c.as_os_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tempdir path as the type the layout takes.
    fn abs(p: impl Into<std::path::PathBuf>) -> AbsPath {
        AbsPath::new(p).expect("a tempdir path is absolute")
    }
    use homma_api::{Identity, Paths, Role};

    fn hand() -> Identity {
        let mut i = Identity::new(Role::Hand, "paja");
        i.git_name = Some("paja".into());
        i.git_email = Some("paja@example.invalid".into());
        i
    }

    fn expert() -> Identity {
        Identity::new(Role::Expert, "proof")
    }

    fn fixture() -> (tempfile::TempDir, Paths) {
        (tempfile::tempdir().unwrap(), Paths::default())
    }

    #[test]
    fn a_hand_and_a_consultant_live_under_different_roots() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        assert!(l.home(&hand()).unwrap().as_path().ends_with("hands/paja"));
        assert!(l
            .home(&expert())
            .unwrap()
            .as_path()
            .ends_with("experts/proof"));
    }

    #[test]
    fn the_harness_link_lands_where_the_harness_looks() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        assert!(l
            .harness_memory(&hand())
            .unwrap()
            .as_path()
            .ends_with(".claude/agent-memory/paja"));
    }

    // The shape below was established by hand before it was built. It is a test
    // rather than a note because a hand check that leaves no test has to be
    // redone by whoever doubts it next.
    #[test]
    fn memory_is_linked_relatively_and_writes_through_to_the_layout() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let id = hand();
        let done = prepare(&l, &id).unwrap();

        let target = fs::read_link(&done.harness_link).unwrap();
        assert!(
            target.is_relative(),
            "an absolute target is a path on one machine, and this link is committed"
        );
        assert_eq!(target, Path::new("../../.shared/hands/paja/memory"));

        // Writing through the harness's path must land in the layout's directory.
        fs::write(
            done.harness_link.as_path().join("MEMORY.md"),
            "learned a thing",
        )
        .unwrap();
        let landed =
            fs::read_to_string(l.memory(&id).unwrap().as_path().join("MEMORY.md")).unwrap();
        assert_eq!(landed, "learned a thing");
    }

    #[test]
    fn preparing_twice_changes_nothing() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let id = hand();
        let first = prepare(&l, &id).unwrap();
        fs::write(first.harness_link.as_path().join("MEMORY.md"), "kept").unwrap();
        prepare(&l, &id).unwrap();
        // Comparing the two Prepared values would pass with prepare a no-op,
        // since they are built from path arithmetic. The content is the test.
        assert_eq!(
            fs::read_to_string(l.memory(&id).unwrap().as_path().join("MEMORY.md")).unwrap(),
            "kept",
            "a second prepare must not clear what the first one's memory holds"
        );
    }

    // The tenth review's finding, at the crate rather than through the binary.
    //
    // `preparing_twice_changes_nothing` above runs against a bare tempdir, where
    // this cannot arise, which is why a crate contradicting itself was invisible
    // at 335 green. Every symlink test on this branch is a *refusal* test; this
    // is the missing *idempotence under a symlink* test.
    //
    // The link body is a path the kernel follows, so it is a path in exactly the
    // sense `Root` exists to prove, and nothing proved it: it was computed
    // lexically between as-written paths while the link is created where those
    // resolve.
    #[test]
    fn the_memory_link_body_stays_inside_a_root_that_carries_a_symlink() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        std::fs::create_dir_all(&root_dir).unwrap();
        // Contained without argument: it resolves to the root. It also removes a
        // level from the real depth, which is the whole reproduction.
        std::os::unix::fs::symlink(".", root_dir.join(".claude")).unwrap();

        // **Resolved**, because that is what the binary passes and because an
        // unresolved one made this test pass for the wrong reason. Measured: with
        // an unresolved root the link directory and the resolved target share no
        // prefix but `/`, so `relative_from` climbs all the way to the root of
        // the filesystem and comes back down, which happens to work. The short
        // broken body only appears once both ends are spelled the same way, and
        // `cmd/mod.rs` canonicalises before `Layout` ever sees the root.
        let root_dir = std::fs::canonicalize(&root_dir).unwrap();
        let p = Paths::default();
        let l = Layout::new(&abs(&root_dir), &p).unwrap();
        let id = hand();
        let done = prepare(&l, &id).expect("preparing must succeed, the link is contained");

        let body = fs::read_link(done.harness_link.as_path()).expect("the link is created");
        let landed = fs::canonicalize(
            done.harness_link
                .as_path()
                .parent()
                .expect("a link has a parent")
                .join(&body),
        )
        .expect("the target resolves");
        let root_real = fs::canonicalize(&root_dir).unwrap();
        assert!(
            landed.starts_with(&root_real),
            "the body {} resolves to {}, outside {}",
            body.display(),
            landed.display(),
            root_real.display()
        );

        // And preparing again must not refuse what the first pass created.
        prepare(&l, &id).expect("a second prepare must not refuse the first pass's own link");
    }

    // The eleventh review's reproduction, and the shape that showed the parent
    // check was live while nothing pinned it. Replacing that check with a bare
    // `create_dir_all` left all 338 tests green and opened this.
    //
    // The trick is that containment on the link itself passes: `resolved`
    // follows the **final** component, and the final component points back
    // inside the root. It is the parent chain that leaves, and the parent is
    // what `remove_file` and `symlink` actually operate in.
    #[test]
    fn a_parent_chain_that_leaves_is_refused_even_when_the_last_link_returns() {
        let d = tempfile::tempdir().unwrap();
        let root_dir = d.path().join("root");
        let outside = d.path().join("outside");
        std::fs::create_dir_all(root_dir.join(".shared/hands/paja/memory")).unwrap();
        std::fs::create_dir_all(outside.join("agent-memory")).unwrap();

        // The parent chain leaves the root.
        std::os::unix::fs::symlink("../outside", root_dir.join(".claude")).unwrap();
        // And the final component comes back in, so the link alone looks fine.
        std::os::unix::fs::symlink(
            root_dir.join(".shared/hands/paja/memory"),
            outside.join("agent-memory").join("paja"),
        )
        .unwrap();

        let p = Paths::default();
        let l = Layout::new(&abs(&root_dir), &p).unwrap();
        let err = prepare(&l, &hand())
            .expect_err("the parent chain leaves the root, whatever the last component does");
        assert!(
            err.to_string().contains("outside the workspace root"),
            "must refuse for the right reason: {err}"
        );

        // And nothing outside was touched on the way to the refusal. Read the
        // link rather than following it: following it lands back inside, which
        // is the whole reason this shape hid.
        let planted = outside.join("agent-memory").join("paja");
        let body = fs::read_link(&planted).expect("the link we planted is still a link");
        assert_eq!(
            body,
            root_dir.join(".shared/hands/paja/memory"),
            "homma must not have replaced a link outside the root"
        );
    }

    #[test]
    fn a_real_directory_where_the_link_belongs_is_refused_rather_than_deleted() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let id = hand();
        let link = l.harness_memory(&id).unwrap();
        fs::create_dir_all(&link).unwrap();
        fs::write(
            link.as_path().join("someones-notes.md"),
            "not ours to delete",
        )
        .unwrap();

        let err = prepare(&l, &id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(
            link.as_path().join("someones-notes.md").exists(),
            "refusing must not be a euphemism for deleting"
        );
    }

    #[test]
    fn a_role_that_does_not_remember_gets_no_memory_directory() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let general = Identity::new(Role::General, "runner");
        let done = prepare(&l, &general).unwrap();
        assert!(
            !done.memory.as_path().exists(),
            "labour accumulates nothing"
        );
        assert!(!done.harness_link.as_path().exists());
    }

    #[test]
    fn a_consultant_gets_memory_and_no_notes() {
        // Notes are a twin's staging area, and a consultant has no prime to
        // triage them.
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let done = prepare(&l, &expert()).unwrap();
        assert!(done.memory.as_path().exists());
        assert!(!done.notes.as_path().exists());
    }

    #[test]
    fn the_twin_definition_is_a_different_file_from_the_primes() {
        let (d, p) = fixture();
        let l = Layout::new(&abs(d.path()), &p).unwrap();
        let id = hand();
        assert_ne!(l.definition(&id).unwrap(), l.twin_definition(&id).unwrap());
    }

    #[test]
    fn a_relative_path_is_computed_between_siblings_and_across_depths() {
        assert_eq!(
            relative_from(Path::new("/a/b/c"), Path::new("/a/x/y")),
            Path::new("../../x/y")
        );
        assert_eq!(
            relative_from(Path::new("/a/b"), Path::new("/a/b/c")),
            Path::new("c")
        );
    }
}
