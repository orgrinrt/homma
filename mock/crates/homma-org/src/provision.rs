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
    /// The directory above the workspace's parent does not exist.
    ///
    /// Its own variant because `io::Error` from `create_dir` carries neither the
    /// path nor a remedy, and the bare message is "No such file or directory
    /// (os error 2)". The whole reason one level is created here is that the
    /// alternative error read as a network fault; an unreadable message at this
    /// layer would be the same defect one layer up.
    ParentMissing { parent: AbsPath, workspace: AbsPath },
    /// A workspace already exists and was cloned from something else.
    WrongRemote {
        expected: String,
        found: Option<String>,
    },
    /// The identity did not survive being written.
    IdentityNotSet {
        found: Option<homma_api::CommitIdentity>,
    },
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
            ProvisionError::ParentMissing { parent, workspace } => write!(
                f,
                "{parent} does not exist, so the workspace {workspace} cannot be \
                 created without building the path to it. homma creates the \
                 workspace's own parent and never a chain of them; make {parent} \
                 first."
            ),
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
                 Name a workspace outside it."
            ),
            ProvisionError::IdentityNotSet { found } => write!(
                f,
                "the identity did not survive being written; the clone reports {}",
                match found {
                    Some(i) => format!(
                        "author {} <{}>, committer {} <{}>",
                        i.author_name, i.author_email, i.committer_name, i.committer_email
                    ),
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
            ProvisionError::ParentMissing { .. } => None,
            _ => None,
        }
    }
}

/// Clone the content repository into an identity's workspace and set its author
/// and committer identities in that clone's own configuration.
///
/// **Cloning is skipped when the workspace already holds a repository**, which
/// is what keeps standing up twice the same answer. Both identities are set
/// either way, because an entry whose email changed should take effect without
/// anyone deleting a workspace to make it.
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
        return Err(ProvisionError::InsideAnotherRepo {
            workspace: root.clone(),
            enclosing,
        });
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
        //
        // **`create_dir`, not `create_dir_all`, and that is the whole guard.**
        // One level is required for the reason above. A chain never was, and
        // `create_dir_all` supplied whatever chain the configured path implied:
        // a workspace at `somewhere/.claude/hands/paja` built `somewhere` and
        // `somewhere/.claude` on the way there, which is deny item three, and
        // spelled with `..` it did so several levels above the root.
        //
        // The workspace is **required** to sit outside the containment root, so
        // no `Root` covers it and none can. This is the same rule as the root's,
        // stated for the other directory homma creates: it creates the thing,
        // never the path to the thing.
        if let Some(parent) = root.parent() {
            match std::fs::create_dir(&parent) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(ProvisionError::ParentMissing {
                        parent: parent.clone(),
                        workspace: root.clone(),
                    })
                }
                Err(e) => return Err(ProvisionError::Parent(e)),
            }
        }
        git.clone_repo(content_repo_url, &root)
            .map_err(ProvisionError::Git)?;
        true
    };
    // The committer defaults to the author, which is every entry but one. The
    // fallback lives here rather than in the type so that "the same" and
    // "deliberately the same" stay distinguishable in the registry file.
    let want = homma_api::CommitIdentity {
        author_name: name.to_string(),
        author_email: email.to_string(),
        committer_name: id.committer_name.as_deref().unwrap_or(name).to_string(),
        committer_email: id.committer_email.as_deref().unwrap_or(email).to_string(),
    };
    git.set_identity(&root, &want)
        .map_err(ProvisionError::Git)?;

    // Read back, because the design says a stood-up clone reports its own email
    // and nothing was checking. `Git::identity` existed for exactly this and had
    // no caller outside its own tests, which is how a write that never reached
    // disk survived a round.
    //
    // **All four, not the author's two.** The comparison checked the author and
    // ignored the committer, so a `set_identity` that wrote the committer
    // nowhere passed the guard whose entire purpose is that a write which never
    // landed does not.
    match git.identity(&root).map_err(ProvisionError::Git)? {
        Some(ref got) if *got == want => {}
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
        identities: std::cell::RefCell<Vec<(AbsPath, homma_api::CommitIdentity)>>,
        /// When set, `set_identity` records the author and drops the committer.
        ///
        /// The shape the widened read-back exists for. Without it, narrowing the
        /// comparison back to the author failed no test, which is the same
        /// unpinned-guard class the widening was fixing.
        committer_writes_vanish: std::cell::Cell<bool>,
        /// When set, `set_identity` reports success and records nothing.
        ///
        /// The read-back exists precisely because a write that never reached
        /// disk survived a round, and no fake could express that, so the guard
        /// shipped pinned by nothing: replacing it with a discard left the whole
        /// suite green. A double that cannot fail the way production fails is a
        /// double that certifies nothing.
        identity_writes_vanish: std::cell::Cell<bool>,
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
        fn set_identity(
            &self,
            path: &AbsPath,
            id: &homma_api::CommitIdentity,
        ) -> Result<(), Never> {
            if self.committer_writes_vanish.get() {
                // Reports success, records the author, loses the committer.
                self.identities.borrow_mut().push((
                    path.clone(),
                    homma_api::CommitIdentity {
                        committer_name: id.author_name.clone(),
                        committer_email: id.author_email.clone(),
                        ..id.clone()
                    },
                ));
                return Ok(());
            }
            if self.identity_writes_vanish.get() {
                // Reports success, records nothing. Exactly the shape the
                // read-back was written for.
                return Ok(());
            }
            self.identities
                .borrow_mut()
                .push((path.clone(), id.clone()));
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
        fn identity(&self, path: &AbsPath) -> Result<Option<homma_api::CommitIdentity>, Never> {
            Ok(self
                .identities
                .borrow()
                .iter()
                .rev()
                .find(|(p, _)| p == path)
                .map(|(_, id)| id.clone()))
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
            Some(homma_api::CommitIdentity {
                author_name: "paja".into(),
                author_email: "paja@example.invalid".into(),
                committer_name: "paja".into(),
                committer_email: "paja@example.invalid".into(),
            })
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
        git.enclosures
            .borrow_mut()
            .push((ws.clone(), victim.clone()));

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
        // No entry: a repository is not inside itself, which is what the
        // contract now answers and what standing up twice depends on.
        let done = provision(&id, &ws, CONTENT, &git).unwrap();
        assert!(!done.cloned);
    }

    #[test]
    fn provisioning_creates_the_workspaces_parent_before_cloning() {
        // gix will not create it, and a clone into a path whose parent is
        // missing fails with an error about opening data, which reads as a
        // network fault. Found by running this against a real repository after
        // every fake-backed test passed.
        //
        // One level, which is what that reason needs.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("crew").join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap();
        assert!(
            ws.parent().unwrap().exists(),
            "the parent must exist before the clone is attempted"
        );
    }

    // The fourth live-but-unpinned guard found on this branch. Replacing the
    // read-back with a discard left 348 tests green, and
    // `ProvisionError::IdentityNotSet` was constructed at one site, asserted by
    // nothing, with a `Display` arm that never executed.
    //
    // Its own comment says it exists because a write that never reached disk
    // survived a round. No fake could express that until now, which is the
    // reason rather than an excuse: a double that cannot fail the way production
    // fails certifies nothing.
    // U-3.2: the registry field reaching the clone. The record settles it for
    // Vouti, whose author is op and whose committer is a tagged address on op's
    // own.
    #[test]
    fn a_distinct_committer_reaches_the_clone() {
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("vouti"));
        let mut id = staffed_hand(&ws);
        id.git_name = Some("Onni Armas".into());
        id.git_email = Some("ort@hiisi.digital".into());
        id.committer_email = Some("orgrinrt+vouti@ikiuni.dev".into());
        let git = FakeGit::default();

        provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap();

        let written = git.identities.borrow();
        let (_, got) = written.last().expect("an identity was set");
        assert_eq!(got.author_name, "Onni Armas");
        assert_eq!(got.author_email, "ort@hiisi.digital", "the author stays op");
        assert_eq!(
            got.committer_email, "orgrinrt+vouti@ikiuni.dev",
            "and the committer is what distinguishes the crew's writes"
        );
        assert_eq!(
            got.committer_name, "Onni Armas",
            "the committer name defaults to the author's when the entry names none"
        );
    }

    // `committer_name` reached nothing when it was added: ignoring the registry
    // field entirely left the whole suite green. It shipped as unread plumbing,
    // which is the exact shape U-3.1 exists to prevent, in the round that added
    // it because the unit named it.
    #[test]
    fn a_distinct_committer_name_reaches_the_clone() {
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("vouti"));
        let mut id = staffed_hand(&ws);
        id.git_name = Some("Onni Armas".into());
        id.git_email = Some("ort@hiisi.digital".into());
        id.committer_name = Some("Vouti".into());
        let git = FakeGit::default();

        provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap();

        let written = git.identities.borrow();
        let (_, got) = written.last().expect("an identity was set");
        assert_eq!(
            got.author_name, "Onni Armas",
            "the author's name is unchanged"
        );
        assert_eq!(
            got.committer_name, "Vouti",
            "and the committer's name is the one the registry gave"
        );
    }

    #[test]
    fn an_entry_with_no_committer_commits_as_its_author() {
        // Every ordinary entry. The fallback lives in `provision` rather than in
        // the type, so the registry file can still distinguish "the same" from
        // "deliberately the same".
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        provision(&id, &ws, "git@example.invalid:x/y.git", &git).unwrap();

        let written = git.identities.borrow();
        let (_, got) = written.last().expect("an identity was set");
        // Against what the registry said, not against each other. Comparing the
        // two halves of one recorded value catches nothing: it holds for any
        // implementation that writes the same thing twice, including one that
        // writes the wrong thing twice.
        assert_eq!(got.author_name, "paja");
        assert_eq!(got.author_email, "paja@example.invalid");
        assert_eq!(got.committer_name, "paja");
        assert_eq!(got.committer_email, "paja@example.invalid");
    }

    // Pins the widened read-back. Narrowing the comparison back to the author
    // failed nothing before this existed, which is the same class the widening
    // was written to close: a guard improved and left unpinned.
    #[test]
    fn a_committer_that_never_reached_the_clone_is_caught() {
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("vouti"));
        let mut id = staffed_hand(&ws);
        id.git_name = Some("Onni Armas".into());
        id.git_email = Some("ort@hiisi.digital".into());
        id.committer_email = Some("orgrinrt+vouti@ikiuni.dev".into());
        let git = FakeGit::default();
        git.committer_writes_vanish.set(true);

        let err = provision(&id, &ws, "git@example.invalid:x/y.git", &git)
            .expect_err("a committer the clone never received is a write that did not land");
        assert!(
            matches!(err, ProvisionError::IdentityNotSet { .. }),
            "must refuse for the right reason: {err}"
        );
    }

    #[test]
    fn an_identity_write_that_reports_success_and_lands_nowhere_is_caught() {
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();
        git.identity_writes_vanish.set(true);

        let err = provision(&id, &ws, "git@example.invalid:x/y.git", &git)
            .expect_err("a Hand that commits as the machine's owner is the failure this stops");
        assert!(
            matches!(err, ProvisionError::IdentityNotSet { .. }),
            "must refuse for the right reason: {err}"
        );
    }

    #[test]
    fn provisioning_refuses_to_build_a_chain_of_directories_to_the_workspace() {
        // **This asserts the opposite of what this test's sibling asserted one
        // round ago**, and the sibling was a correct test of a rule that had to
        // change. Ten tests failed when the creation was deleted and every one
        // of them was pinning the behaviour being corrected, which is worth
        // remembering: a guard being pinned says nothing about whether what it
        // pins is wanted.
        let d = tempfile::tempdir().unwrap();
        let ws = abs(d.path().join("nested").join("deeper").join("paja"));
        let id = staffed_hand(&ws);
        let git = FakeGit::default();

        let err = provision(&id, &ws, "git@example.invalid:x/y.git", &git)
            .expect_err("two missing levels is a chain, and homma does not build one");
        assert!(
            matches!(err, ProvisionError::ParentMissing { .. }),
            "must refuse for the right reason: {err}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("nested") && msg.contains("make"),
            "a refusal has to name the directory and say what to do: {msg}"
        );
        assert!(
            !d.path().join("nested").exists(),
            "and must not have built the first level on the way to refusing"
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
