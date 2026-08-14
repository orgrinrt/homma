//! A `Git` that answers from tables a test fills in.
//!
//! Its own module because `stand.rs` crossed the file-size limit carrying it,
//! and because a fake used by more than one test module wants one definition
//! rather than two that drift apart.
//!
//! **It deliberately asserts nothing about its arguments.** An earlier version
//! asserted that paths were absolute, which the parameter type already
//! guarantees. The assertion was a tautology, nothing ever executed it, and two
//! successive rounds cited it as evidence of a property it did not establish.
//! A fake earns its place by making a branch reachable, not by restating the
//! signature.

#![cfg(test)]

use homma_api::{AbsPath, Git};
use std::path::PathBuf;

pub const CONTENT: &str = "git@example.invalid:orgrinrt/clause-dev.git";

pub struct FakeGit {
    /// What `origin_url` answers for the workspace root.
    pub root_origin: Option<String>,
    pub cloned: std::cell::RefCell<Vec<(String, AbsPath)>>,
    pub identities: std::cell::RefCell<Vec<(AbsPath, String, String, String, String)>>,
    /// A path, and the repository it sits inside. Empty means nothing is
    /// nested, which is what most tests want.
    pub enclosures: std::cell::RefCell<Vec<(AbsPath, AbsPath)>>,
}

impl FakeGit {
    /// A root that is a clone of the content repository. The ordinary case.
    pub fn at_the_content_repo() -> Self {
        Self {
            root_origin: Some(CONTENT.into()),
            cloned: Default::default(),
            identities: Default::default(),
            enclosures: Default::default(),
        }
    }

    /// A root that is a clone of something else entirely.
    pub fn somewhere_else() -> Self {
        Self {
            root_origin: Some("git@example.invalid:orgrinrt/member.git".into()),
            cloned: Default::default(),
            identities: Default::default(),
            enclosures: Default::default(),
        }
    }

    /// A root that is not a repository at all.
    pub fn no_origin() -> Self {
        Self {
            root_origin: None,
            cloned: Default::default(),
            identities: Default::default(),
            enclosures: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct Never;

impl std::fmt::Display for Never {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "never")
    }
}

impl std::error::Error for Never {}

impl Git for FakeGit {
    type Error = Never;

    fn is_repo(&self, path: &AbsPath) -> bool {
        self.cloned.borrow().iter().any(|(_, p)| p == path)
    }

    fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Never> {
        self.cloned
            .borrow_mut()
            .push((url.to_string(), dest.clone()));
        Ok(())
    }

    fn init(&self, _path: &AbsPath) -> Result<(), Never> {
        Ok(())
    }

    fn set_identity(
        &self,
        path: &AbsPath,
        name: &str,
        email: &str,
        committer_name: &str,
        committer: &str,
    ) -> Result<(), Never> {
        // Every argument is recorded rather than dropped. A double that discards
        // half its argument cannot fail the way production fails, which is how a
        // guard on this branch went four rounds pinned by nothing.
        //
        // The crate that owns this fake asserts the author path only; the
        // committer is asserted in `homma-org`, where `provision` lives. The
        // fields are here so a future test in this crate can, and a review found
        // an earlier comment claiming one already did.
        self.identities.borrow_mut().push((
            path.clone(),
            name.to_string(),
            email.to_string(),
            committer_name.to_string(),
            committer.to_string(),
        ));
        Ok(())
    }

    fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Never> {
        Ok(self
            .enclosures
            .borrow()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, e)| e.clone()))
    }

    fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Never> {
        // A cloned workspace reports what it was cloned from; anything else is
        // the root, and reports what this fake was built to say.
        Ok(self
            .cloned
            .borrow()
            .iter()
            .find(|(_, p)| p == path)
            .map(|(u, _)| u.clone())
            .or_else(|| self.root_origin.clone()))
    }

    fn identity(&self, path: &AbsPath) -> Result<Option<homma_api::CommitIdentity>, Never> {
        Ok(self
            .identities
            .borrow()
            .iter()
            .rev()
            .find(|(p, _, _, _, _)| p == path)
            .map(|(_, n, e, cn, ce)| homma_api::CommitIdentity {
                author_name: n.clone(),
                author_email: e.clone(),
                committer_name: cn.clone(),
                committer_email: ce.clone(),
            }))
    }
}

/// A tempdir path as the type the contract takes.
pub fn abs(p: impl Into<PathBuf>) -> AbsPath {
    AbsPath::new(p).expect("a tempdir path is absolute")
}
