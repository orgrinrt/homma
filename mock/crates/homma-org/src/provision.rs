//! Cloning a content repository into a workspace and giving it an identity.
//!
//! Split from `workspace.rs`, which crossed the file-size limit when this
//! landed. The two are different jobs: that one arranges directories homma owns,
//! this one runs git against a tree it does not.

use crate::remote::same_repo;
use homma_api::{AbsPath, Git, Identity};

/// What provisioning a workspace did, so a caller reports rather than guesses.
#[derive(Debug, PartialEq, Eq)]
pub struct Provisioned {
    /// Where the workspace is.
    pub root: AbsPath,
    /// False when the workspace already held the content repository.
    pub cloned: bool,
}

/// Why a workspace could not be provisioned.
#[derive(Debug)]
pub enum ProvisionError<E> {
    /// The entry carries no git identity, so a clone would commit as whoever
    /// this machine belongs to.
    NoIdentity,
    /// The git operation failed.
    Git(E),
    /// The workspace's parent directory could not be created.
    Parent(std::io::Error),
    /// A workspace already exists and was cloned from something else.
    WrongRemote {
        expected: String,
        found: Option<String>,
    },
    /// The identity did not survive being written.
    IdentityNotSet { found: Option<(String, String)> },
    /// The workspace would sit inside a repository that is not it.
    InsideAnotherRepo {
        workspace: homma_api::AbsPath,
        enclosing: homma_api::AbsPath,
    },
}

impl<E: std::fmt::Display> std::fmt::Display for ProvisionError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::NoIdentity => write!(f, "the entry carries no git identity"),
            ProvisionError::Git(e) => write!(f, "git: {e}"),
            ProvisionError::Parent(e) => write!(f, "creating the parent directory: {e}"),
            ProvisionError::WrongRemote { expected, found } => write!(
                f,
                "the workspace is already a clone of {}, not of {expected}. \
                 Standing up again will not fix it; move or remove that \
                 workspace first.",
                found.as_deref().unwrap_or("nothing with an origin")
            ),
            ProvisionError::InsideAnotherRepo {
                workspace,
                enclosing,
            } => write!(
                f,
                "{workspace} sits inside the repository at {enclosing}. Creating \
                 a workspace there would write into a tree that is not ours, \
                 which the deny list forbids. Name a workspace outside it."
            ),
            ProvisionError::IdentityNotSet { found } => write!(
                f,
                "the identity did not survive being written; the clone reports {}",
                match found {
                    Some((n, e)) => format!("{n} <{e}>"),
                    None => "none".to_string(),
                }
            ),
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
    workspace: &AbsPath,
    content_repo_url: &str,
    git: &G,
) -> Result<Provisioned, ProvisionError<G::Error>> {
    // The workspace arrives already resolved. It used to be built here with
    // `PathBuf::from(id.workspace)`, so a registry saying `workspace =
    // "hands/rel"` cloned into whatever directory the process was in and wrote
    // a nested repository into whatever tree that was.
    let root = workspace.clone();
    let (name, email) = match (&id.git_name, &id.git_email) {
        (Some(n), Some(e)) => (n, e),
        // Refused rather than cloning and leaving the identity for later. A
        // workspace without one commits as the machine's owner, and the first
        // sign of that is a commit already made.
        _ => return Err(ProvisionError::NoIdentity),
    };

    // **The check that was on the wrong path.** `enclosing_repo` answers "is
    // this inside somebody's repository", which is the question, and it was
    // being asked about the workspace root rather than about the thing being
    // created. A relative `..`, and an absolute path pointing straight into
    // another tree, both reach the same place and neither is stopped by the
    // path being absolute.
    //
    // A workspace that is already the repository is fine: that is standing up
    // twice. What is refused is one nested inside a different repository.
    if let Some(enclosing) = git.enclosing_repo(&root).map_err(ProvisionError::Git)? {
        if enclosing != root {
            return Err(ProvisionError::InsideAnotherRepo {
                workspace: root.clone(),
                enclosing,
            });
        }
    }

    let cloned = if git.is_repo(&root) {
        // Checked rather than trusted. A workspace cloned from the wrong
        // repository is otherwise permanent: a later run reports it already
        // present and exits 0, so the mistake never surfaces again.
        let found = git.origin_url(&root).map_err(ProvisionError::Git)?;
        match found {
            Some(ref url) if same_repo(url, content_repo_url) => false,
            other => {
                return Err(ProvisionError::WrongRemote {
                    expected: content_repo_url.to_string(),
                    found: other,
                })
            }
        }
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

    // Read back, because the design says a stood-up clone reports its own email
    // and nothing was checking. `Git::identity` existed for exactly this and had
    // no caller outside its own tests, which is how a write that never reached
    // disk survived a round.
    match git.identity(&root).map_err(ProvisionError::Git)? {
        Some((ref n, ref e)) if n == name && e == email => {}
        found => return Err(ProvisionError::IdentityNotSet { found }),
    }

    Ok(Provisioned { root, cloned })
}

#[cfg(test)]
mod tests {
    use super::*;
    use homma_api::Role;
    use std::path::PathBuf;

    fn abs(p: impl Into<PathBuf>) -> AbsPath {
        AbsPath::new(p).expect("a tempdir path is absolute")
    }

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
        /// What `origin_url` answers for a given path. A fake answering one URL
        /// for every path cannot enter the refusing arm, which is why the guard
        /// shipped with no test.
        remotes: std::cell::RefCell<Vec<(AbsPath, String)>>,
        /// A path, and the repository it sits inside.
        enclosures: std::cell::RefCell<Vec<(AbsPath, AbsPath)>>,
        existing: std::cell::RefCell<Vec<AbsPath>>,
        clones: std::cell::RefCell<Vec<(String, AbsPath)>>,
        identities: std::cell::RefCell<Vec<(AbsPath, String, String)>>,
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
        fn is_repo(&self, path: &AbsPath) -> bool {
            self.existing.borrow().iter().any(|p| p == path)
        }
        fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Never> {
            self.clones
                .borrow_mut()
                .push((url.to_string(), dest.clone()));
            self.existing.borrow_mut().push(dest.clone());
            Ok(())
        }
        fn set_identity(&self, path: &AbsPath, name: &str, email: &str) -> Result<(), Never> {
            self.identities
                .borrow_mut()
                .push((path.clone(), name.to_string(), email.to_string()));
            Ok(())
        }
        fn init(&self, _path: &AbsPath) -> Result<(), Never> {
            Ok(())
        }
        fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Never> {
            // Answers from a table the test fills in, so the refusing arm is
            // reachable. The assertion that used to be here asserted what the
            // parameter type guarantees and was never executed by anything.
            Ok(self
                .enclosures
                .borrow()
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, e)| e.clone()))
        }
        fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Never> {
            if let Some((_, url)) = self.remotes.borrow().iter().find(|(p, _)| p == path) {
                return Ok(Some(url.clone()));
            }
            Ok(self
                .clones
                .borrow()
                .iter()
                .find(|(_, p)| p == path)
                .map(|(u, _)| u.clone()))
        }
        fn identity(&self, path: &AbsPath) -> Result<Option<(String, String)>, Never> {
            Ok(self
                .identities
                .borrow()
                .iter()
                .rev()
                .find(|(p, _, _)| p == path)
                .map(|(_, n, e)| (n.clone(), e.clone())))
        }
    }

    fn staffed_hand(at: &AbsPath) -> Identity {
        let mut i = hand();
        i.staffed = true;
        i.workspace = Some(at.to_string());
        i
    }

    #[test]
    fn provisioning_clones_and_sets_the_identity_in_that_clone() {
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        let done = provision(&id, &ws, "git@example.invalid:orgrinrt/content.git", &git).unwrap();
        assert!(done.cloned);
        assert_eq!(git.clones.borrow().len(), 1);
        assert_eq!(
            git.identity(&ws).unwrap(),
            Some(("paja".into(), "paja@example.invalid".into()))
        );
    }

    #[test]
    fn a_workspace_inside_another_repository_is_refused() {
        // The escape the type did not close. `AbsPath` carries absoluteness and
        // no containment, so `workspace = "../victim/stolen"` resolved to an
        // absolute path pointing into an unrelated committed repository, and a
        // repository was cloned into its working tree. Exit 0.
        let d = tempfile::tempdir().unwrap();
        let victim = abs(d.path().join("victim"));
        let ws = abs(d.path().join("victim").join("stolen"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.enclosures.borrow_mut().push((ws.clone(), victim.clone()));

        match provision(&id, &ws, CONTENT, &git).unwrap_err() {
            ProvisionError::InsideAnotherRepo {
                workspace,
                enclosing,
            } => {
                assert_eq!(workspace, ws);
                assert_eq!(enclosing, victim);
            }
            other => panic!("must refuse, got {other:?}"),
        }
        assert!(
            git.clones.borrow().is_empty(),
            "refusing must happen before anything is cloned"
        );
    }

    #[test]
    fn a_workspace_that_is_itself_the_repository_is_not_refused() {
        // Standing up twice. The guard is about being nested in a *different*
        // repository, and refusing this would break the idempotence the whole
        // lifecycle rests on.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());
        git.remotes.borrow_mut().push((ws.clone(), CONTENT.into()));
        git.enclosures.borrow_mut().push((ws.clone(), ws.clone()));

        let done = provision(&id, &ws, CONTENT, &git).unwrap();
        assert!(!done.cloned);
    }

    #[test]
    fn provisioning_creates_the_workspaces_parent_before_cloning() {
        // gix will not create it, and a clone into a path whose parent is
        // missing fails with an error about opening data, which reads as a
        // network fault. Found by running this against a real repository after
        // every fake-backed test passed.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("nested").join("deeper").join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap();
        assert!(
            ws.parent().unwrap().exists(),
            "the parent must exist before the clone is attempted"
        );
    }

    const CONTENT: &str = "git@example.invalid:orgrinrt/content.git";

    #[test]
    fn a_workspace_cloned_from_the_wrong_repository_is_refused_not_skipped() {
        // The guard this round is named for. Deleting it entirely left the
        // whole suite green, because the only test touching the skip path used
        // a fake that answered the matching URL for every path, so the refusing
        // arm was unreachable.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());
        git.remotes.borrow_mut().push((
            ws.clone(),
            "git@example.invalid:someone-else/content.git".into(),
        ));

        let err = provision(&id, &ws, CONTENT, &git).unwrap_err();
        match err {
            ProvisionError::WrongRemote { expected, found } => {
                assert_eq!(expected, CONTENT);
                assert_eq!(
                    found.as_deref(),
                    Some("git@example.invalid:someone-else/content.git")
                );
            }
            other => panic!("must refuse with the remote it found, got {other:?}"),
        }
        assert!(
            git.identities.borrow().is_empty(),
            "a refused workspace must not have an identity written into it"
        );
    }

    #[test]
    fn a_same_named_repository_under_another_owner_is_still_the_wrong_one() {
        // Comparing the last path segment made these equal. For a fork or a
        // mirror this is the ordinary case.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());
        git.remotes
            .borrow_mut()
            .push((ws.clone(), "git@example.invalid:a-fork/content.git".into()));

        assert!(matches!(
            provision(&id, &ws, CONTENT, &git).unwrap_err(),
            ProvisionError::WrongRemote { .. }
        ));
    }

    #[test]
    fn a_workspace_with_no_remote_at_all_is_refused_rather_than_adopted() {
        // An existing directory that is a repository and points nowhere is not
        // the content repository, and adopting it would be a guess.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());

        match provision(&id, &ws, CONTENT, &git).unwrap_err() {
            ProvisionError::WrongRemote { found, .. } => assert!(found.is_none()),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn provisioning_a_workspace_that_already_has_the_repo_does_not_clone_over_it() {
        // Standing up twice is the same answer, and this is the half of that
        // property the clone introduces.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.existing.borrow_mut().push(ws.clone());
        git.remotes.borrow_mut().push((ws.clone(), CONTENT.into()));

        let done = provision(&id, &ws, CONTENT, &git).unwrap();
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
        let ws = abs(d.path().join("nameless"));
        let mut id = staffed_hand(&ws);
        id.git_email = None;
        let git = FakeGit::default();

        let err = provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap_err();
        assert!(matches!(err, ProvisionError::NoIdentity));
        assert!(
            git.clones.borrow().is_empty(),
            "refusing must happen before the clone, not after"
        );
    }
}
