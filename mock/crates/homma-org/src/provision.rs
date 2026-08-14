//! Cloning a content repository into a workspace and giving it an identity.
//!
//! Split from `workspace.rs`, which crossed the file-size limit when this
//! landed. The two are different jobs: that one arranges directories homma owns,
//! this one runs git against a tree it does not.

use homma_api::{Git, Identity};
use std::path::PathBuf;

/// What provisioning a workspace did, so a caller reports rather than guesses.
#[derive(Debug, PartialEq, Eq)]
pub struct Provisioned {
    /// Where the workspace is.
    pub root: PathBuf,
    /// False when the workspace already held the content repository.
    pub cloned: bool,
}

/// Why a workspace could not be provisioned.
#[derive(Debug)]
pub enum ProvisionError<E> {
    /// The entry names no workspace, so there is nowhere to put one.
    NoWorkspace,
    /// The entry carries no git identity, so a clone would commit as whoever
    /// this machine belongs to.
    NoIdentity,
    /// The git operation failed.
    Git(E),
    /// The workspace's parent directory could not be created.
    Parent(std::io::Error),
}

impl<E: std::fmt::Display> std::fmt::Display for ProvisionError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::NoWorkspace => write!(f, "the entry names no workspace"),
            ProvisionError::NoIdentity => write!(f, "the entry carries no git identity"),
            ProvisionError::Git(e) => write!(f, "git: {e}"),
            ProvisionError::Parent(e) => write!(f, "creating the parent directory: {e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for ProvisionError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProvisionError::Git(e) => Some(e),
            ProvisionError::Parent(e) => Some(e),
            _ => None,
        }
    }
}

/// Clone the content repository into an identity's workspace and set its git
/// identity in that clone's own configuration.
///
/// **Cloning is skipped when the workspace already holds a repository**, which
/// is what keeps standing up twice the same answer. The identity is set either
/// way, because an entry whose email changed should take effect without anyone
/// deleting a workspace to make it.
pub fn provision<G: Git>(
    id: &Identity,
    content_repo_url: &str,
    git: &G,
) -> Result<Provisioned, ProvisionError<G::Error>> {
    let root = PathBuf::from(id.workspace.as_ref().ok_or(ProvisionError::NoWorkspace)?);
    let (name, email) = match (&id.git_name, &id.git_email) {
        (Some(n), Some(e)) => (n, e),
        // Refused rather than cloning and leaving the identity for later. A
        // workspace without one commits as the machine's owner, and the first
        // sign of that is a commit already made.
        _ => return Err(ProvisionError::NoIdentity),
    };

    let cloned = if git.is_repo(&root) {
        false
    } else {
        // gix will not create the parent, and a clone into a path whose parent
        // is missing fails with an error about opening data, which reads as a
        // network problem and is not one. Found by running this against a real
        // repository; the fake in these tests never touched a filesystem.
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent).map_err(ProvisionError::Parent)?;
        }
        git.clone_repo(content_repo_url, &root)
            .map_err(ProvisionError::Git)?;
        true
    };
    git.set_identity(&root, name, email)
        .map_err(ProvisionError::Git)?;
    Ok(Provisioned { root, cloned })
}

#[cfg(test)]
mod tests {
    use super::*;
    use homma_api::Role;
    use std::path::Path;

    fn hand() -> Identity {
        let mut i = Identity::new(Role::Hand, "paja");
        i.git_name = Some("paja".into());
        i.git_email = Some("paja@example.invalid".into());
        i
    }

    /// Records what it was asked to do. This checks the lifecycle's own logic
    /// and nothing about git; whether an identity actually lands in a config
    /// file is tested against a real repository where the real implementation
    /// lives, because a fake would answer that question by construction.
    #[derive(Default)]
    struct FakeGit {
        existing: std::cell::RefCell<Vec<PathBuf>>,
        clones: std::cell::RefCell<Vec<(String, PathBuf)>>,
        identities: std::cell::RefCell<Vec<(PathBuf, String, String)>>,
    }

    #[derive(Debug)]
    struct Never;
    impl std::fmt::Display for Never {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "never")
        }
    }
    impl std::error::Error for Never {}

    impl Git for FakeGit {
        type Error = Never;
        fn is_repo(&self, path: &Path) -> bool {
            self.existing.borrow().iter().any(|p| p == path)
        }
        fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), Never> {
            self.clones
                .borrow_mut()
                .push((url.to_string(), dest.to_path_buf()));
            self.existing.borrow_mut().push(dest.to_path_buf());
            Ok(())
        }
        fn set_identity(&self, path: &Path, name: &str, email: &str) -> Result<(), Never> {
            self.identities.borrow_mut().push((
                path.to_path_buf(),
                name.to_string(),
                email.to_string(),
            ));
            Ok(())
        }
        fn origin_url(&self, _path: &Path) -> Result<Option<String>, Never> {
            Ok(Some("git@example.invalid:orgrinrt/content.git".into()))
        }
        fn identity(&self, path: &Path) -> Result<Option<(String, String)>, Never> {
            Ok(self
                .identities
                .borrow()
                .iter()
                .rev()
                .find(|(p, _, _)| p == path)
                .map(|(_, n, e)| (n.clone(), e.clone())))
        }
    }

    fn staffed_hand(at: &Path) -> Identity {
        let mut i = hand();
        i.staffed = true;
        i.workspace = Some(at.to_string_lossy().into_owned());
        i
    }

    #[test]
    fn provisioning_clones_and_sets_the_identity_in_that_clone() {
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("paja");
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        let done = provision(&id, "git@example.invalid:orgrinrt/content.git", &git).unwrap();
        assert!(done.cloned);
        assert_eq!(git.clones.borrow().len(), 1);
        assert_eq!(
            git.identity(&ws).unwrap(),
            Some(("paja".into(), "paja@example.invalid".into()))
        );
    }

    #[test]
    fn provisioning_creates_the_workspaces_parent_before_cloning() {
        // gix will not create it, and a clone into a path whose parent is
        // missing fails with an error about opening data, which reads as a
        // network fault. Found by running this against a real repository after
        // every fake-backed test passed.
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("nested").join("deeper").join("paja");
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        provision(&id, "git@example.invalid:x/y.git", &git).unwrap();
        assert!(
            ws.parent().unwrap().exists(),
            "the parent must exist before the clone is attempted"
        );
    }

    #[test]
    fn provisioning_a_workspace_that_already_has_the_repo_does_not_clone_over_it() {
        // Standing up twice is the same answer, and this is the half of that
        // property the clone introduces.
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("paja");
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());

        let done = provision(&id, "git@example.invalid:orgrinrt/content.git", &git).unwrap();
        assert!(!done.cloned);
        assert!(
            git.clones.borrow().is_empty(),
            "an existing workspace must not be cloned over"
        );
        // The identity is still set, so a changed email takes effect without
        // anybody deleting a workspace to make it.
        assert_eq!(git.identities.borrow().len(), 1);
    }

    #[test]
    fn provisioning_without_a_git_identity_is_refused_before_anything_is_cloned() {
        // A clone without an identity commits as whoever this machine belongs
        // to, and the first sign of that is a commit already made.
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("nameless");
        let mut id = staffed_hand(&ws);
        id.git_email = None;
        let git = FakeGit::default();

        let err = provision(&id, "git@example.invalid:x/y.git", &git).unwrap_err();
        assert!(matches!(err, ProvisionError::NoIdentity));
        assert!(
            git.clones.borrow().is_empty(),
            "refusing must happen before the clone, not after"
        );
    }

    #[test]
    fn provisioning_an_entry_naming_no_workspace_is_refused() {
        let id = hand();
        let git = FakeGit::default();
        assert!(matches!(
            provision(&id, "git@example.invalid:x/y.git", &git).unwrap_err(),
            ProvisionError::NoWorkspace
        ));
    }
}
