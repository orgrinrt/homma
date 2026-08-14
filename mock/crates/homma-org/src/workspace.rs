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

use homma_api::{Identity, Paths};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Where things sit inside one workspace.
pub struct Layout<'a> {
    root: PathBuf,
    paths: &'a Paths,
}

impl<'a> Layout<'a> {
    pub fn new(root: impl Into<PathBuf>, paths: &'a Paths) -> Self {
        Self {
            root: root.into(),
            paths,
        }
    }

    /// Everything belonging to one identity lives under one directory.
    pub fn home(&self, id: &Identity) -> PathBuf {
        let base = if id.role.has_workspace() {
            &self.paths.hands
        } else {
            &self.paths.experts
        };
        self.root.join(base).join(&id.handle)
    }

    pub fn memory(&self, id: &Identity) -> PathBuf {
        self.home(id).join("memory")
    }

    pub fn notes(&self, id: &Identity) -> PathBuf {
        self.home(id).join("notes")
    }

    pub fn character(&self, id: &Identity) -> PathBuf {
        self.home(id).join("character.md")
    }

    /// Where the harness expects to find this identity's memory.
    pub fn harness_memory(&self, id: &Identity) -> PathBuf {
        self.root
            .join(".claude")
            .join("agent-memory")
            .join(&id.handle)
    }

    pub fn definition(&self, id: &Identity) -> PathBuf {
        self.root
            .join(&self.paths.agents)
            .join(format!("{}.md", id.handle))
    }

    /// The definition a twin runs under.
    ///
    /// A separate file because the restriction that a twin may not write memory
    /// has to be structural: a definition carrying the memory key grants the
    /// write path whatever its prose says, so the twin's simply does not carry
    /// it.
    pub fn twin_definition(&self, id: &Identity) -> PathBuf {
        self.root
            .join(&self.paths.agents)
            .join(format!("{}-twin.md", id.handle))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// What was done to stand an identity up, so a caller can report it rather than
/// guess.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepared {
    pub home: PathBuf,
    pub memory: PathBuf,
    pub notes: PathBuf,
    pub harness_link: PathBuf,
    pub definition: PathBuf,
    pub twin_definition: PathBuf,
}

/// Create the directories an identity needs and link its memory where the
/// harness will look for it.
///
/// Idempotent: running it against a workspace that already has them changes
/// nothing and returns the same answer.
pub fn prepare(layout: &Layout<'_>, id: &Identity) -> io::Result<Prepared> {
    let home = layout.home(id);
    let memory = layout.memory(id);
    let notes = layout.notes(id);
    if id.role.has_memory() {
        fs::create_dir_all(&memory)?;
    }
    if id.role.has_workspace() {
        fs::create_dir_all(&notes)?;
    }

    let link = layout.harness_memory(id);
    if id.role.has_memory() {
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        link_memory(&link, &memory)?;
    }

    if let Some(parent) = layout.definition(id).parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(Prepared {
        home,
        memory,
        notes,
        harness_link: link,
        definition: layout.definition(id),
        twin_definition: layout.twin_definition(id),
    })
}

/// Point `link` at `target`, relatively, replacing an existing link.
///
/// Relative because an absolute target is a path on one machine, and the link is
/// committed: a clone elsewhere must resolve it.
fn link_memory(link: &Path, target: &Path) -> io::Result<()> {
    let relative = relative_from(link.parent().unwrap_or(Path::new(".")), target);
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
        let l = Layout::new(d.path(), &p);
        assert!(l.home(&hand()).ends_with("hands/paja"));
        assert!(l.home(&expert()).ends_with("experts/proof"));
    }

    #[test]
    fn the_harness_link_lands_where_the_harness_looks() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        assert!(l
            .harness_memory(&hand())
            .ends_with(".claude/agent-memory/paja"));
    }

    // The shape below was established by hand before it was built. It is a test
    // rather than a note because a hand check that leaves no test has to be
    // redone by whoever doubts it next.
    #[test]
    fn memory_is_linked_relatively_and_writes_through_to_the_layout() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        let done = prepare(&l, &id).unwrap();

        let target = fs::read_link(&done.harness_link).unwrap();
        assert!(
            target.is_relative(),
            "an absolute target is a path on one machine, and this link is committed"
        );
        assert_eq!(target, Path::new("../../.shared/hands/paja/memory"));

        // Writing through the harness's path must land in the layout's directory.
        fs::write(done.harness_link.join("MEMORY.md"), "learned a thing").unwrap();
        let landed = fs::read_to_string(l.memory(&id).join("MEMORY.md")).unwrap();
        assert_eq!(landed, "learned a thing");
    }

    #[test]
    fn preparing_twice_changes_nothing() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        let first = prepare(&l, &id).unwrap();
        fs::write(first.harness_link.join("MEMORY.md"), "kept").unwrap();
        let second = prepare(&l, &id).unwrap();
        // Comparing the two Prepared values would pass with prepare a no-op,
        // since they are built from path arithmetic. The content is the test.
        assert_eq!(
            fs::read_to_string(l.memory(&id).join("MEMORY.md")).unwrap(),
            "kept",
            "a second prepare must not clear what the first one's memory holds"
        );
    }

    #[test]
    fn a_real_directory_where_the_link_belongs_is_refused_rather_than_deleted() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        let link = l.harness_memory(&id);
        fs::create_dir_all(&link).unwrap();
        fs::write(link.join("someones-notes.md"), "not ours to delete").unwrap();

        let err = prepare(&l, &id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(
            link.join("someones-notes.md").exists(),
            "refusing must not be a euphemism for deleting"
        );
    }

    #[test]
    fn a_role_that_does_not_remember_gets_no_memory_directory() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let general = Identity::new(Role::General, "runner");
        let done = prepare(&l, &general).unwrap();
        assert!(!done.memory.exists(), "labour accumulates nothing");
        assert!(!done.harness_link.exists());
    }

    #[test]
    fn a_consultant_gets_memory_and_no_notes() {
        // Notes are a twin's staging area, and a consultant has no prime to
        // triage them.
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let done = prepare(&l, &expert()).unwrap();
        assert!(done.memory.exists());
        assert!(!done.notes.exists());
    }

    #[test]
    fn the_twin_definition_is_a_different_file_from_the_primes() {
        let (d, p) = fixture();
        let l = Layout::new(d.path(), &p);
        let id = hand();
        assert_ne!(l.definition(&id), l.twin_definition(&id));
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
