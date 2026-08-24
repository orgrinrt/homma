//! What standing a workspace up needs from git, as a contract rather than an
//! implementation.
//!
//! **Every path here is an [`AbsPath`]**, which is why no method checks whether
//! it was given a relative one. That precondition was prose, then a runtime
//! check in a single implementation, and both let three consecutive rounds ship
//! a route that walked out of it. In the signature it is checked once, by the
//! compiler.
//!
//! Declared here because it is vocabulary: the registry crate has to say "clone
//! this and set that identity" without knowing which git library says it. No I/O
//! happens in this module, which is the rule for this crate; a trait describing
//! I/O is not performing any.
//!
//! The reason this is a trait at all, rather than a direct call into the git
//! crate, is that the lifecycle and the git operations fail differently and want
//! testing differently. The lifecycle wants a fake, so its own logic can be
//! checked without a repository. The implementation wants a real repository,
//! because a fake proves the wiring and says nothing about whether the identity
//! actually landed.

use crate::path::AbsPath;

/// What a clone is configured to commit as.
///
/// Named `CommitIdentity` rather than `Identity` because `config::Identity` is
/// the registry entry and these are different things: one is who a participant
/// is, the other is what a particular clone will stamp on a commit.
/// **The four parts are private and every one of them is non-empty.** An empty
/// name is not a name, and `set_identity` writes six keys from these, so a
/// clone built from an empty part commits as nobody. Reading an identity out of
/// a clone already filtered empties; publishing the fields meant that care
/// covered one way of obtaining one and no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitIdentity {
    author_name:     String,
    author_email:    String,
    committer_name:  String,
    committer_email: String,
}

/// Which part of a [`CommitIdentity`] a refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    AuthorName,
    AuthorEmail,
    CommitterName,
    CommitterEmail,
}

impl Part {
    /// The git configuration key this part is written to.
    pub fn key(self) -> &'static str {
        match self {
            Self::AuthorName => "author.name",
            Self::AuthorEmail => "author.email",
            Self::CommitterName => "committer.name",
            Self::CommitterEmail => "committer.email",
        }
    }
}

impl std::fmt::Display for Part {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

/// A part of an identity was empty, and which one.
///
/// Carries the part rather than only the fact, because an operator handed "a
/// value was empty" has to work out which of six keys to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyPart(pub Part);

impl std::fmt::Display for EmptyPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is empty, and an empty value is not a value", self.0)
    }
}

impl std::error::Error for EmptyPart {}

impl CommitIdentity {
    /// One identity in both roles, which is every ordinary entry.
    pub fn same(name: impl Into<String>, email: impl Into<String>) -> Result<Self, EmptyPart> {
        let (name, email) = (name.into(), email.into());
        Self::split(name.clone(), email.clone(), name, email)
    }

    /// An author and a committer that differ, which is the case one clone
    /// carrying two identities exists for.
    pub fn split(
        author_name: impl Into<String>,
        author_email: impl Into<String>,
        committer_name: impl Into<String>,
        committer_email: impl Into<String>,
    ) -> Result<Self, EmptyPart> {
        let it = Self {
            author_name:     author_name.into(),
            author_email:    author_email.into(),
            committer_name:  committer_name.into(),
            committer_email: committer_email.into(),
        };
        for (part, value) in [
            (Part::AuthorName, &it.author_name),
            (Part::AuthorEmail, &it.author_email),
            (Part::CommitterName, &it.committer_name),
            (Part::CommitterEmail, &it.committer_email),
        ] {
            if value.trim().is_empty() {
                return Err(EmptyPart(part));
            }
        }
        Ok(it)
    }

    /// The name commits are authored by.
    pub fn author_name(&self) -> &str {
        &self.author_name
    }

    /// The address commits are authored by.
    pub fn author_email(&self) -> &str {
        &self.author_email
    }

    /// The name commits are committed by, which equals the author's for every
    /// identity built with [`CommitIdentity::same`].
    pub fn committer_name(&self) -> &str {
        &self.committer_name
    }

    /// The address commits are committed by.
    pub fn committer_email(&self) -> &str {
        &self.committer_email
    }
}

/// The git operations a workspace lifecycle performs.
pub trait Git {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether `path` already holds a repository.
    ///
    /// Standing up twice is the same answer, and this is what makes that true
    /// for the clone: a workspace that already has the content repository is
    /// left alone rather than re-cloned over.
    fn is_repo(&self, path: &AbsPath) -> bool;

    /// Clone `url` into `dest`, which does not yet exist as a repository.
    fn clone_repo(&self, url: &str, dest: &AbsPath) -> Result<(), Self::Error>;

    /// Set the clone's own author and committer, name and address for each.
    ///
    /// **Two identities, because one clone legitimately carries two.** The
    /// record settles it for Vouti: the author stays op, and the committer is a
    /// tagged address on op's own so it "just works" while distinguishing what
    /// the crew wrote.
    ///
    /// **Six keys, and the names are not optional extras.** Git resolves an
    /// identity from `author.name`, `author.email`, `committer.name` and
    /// `committer.email`, falling back to `user.name` and `user.email` for
    /// whichever is unset, and it does so **across scopes**: a global
    /// `author.name` beats a local `user.name`. Writing only the `user.*` pair
    /// therefore left a provisioned workspace committing under whatever the
    /// machine's global configuration said, on any machine that has one.
    ///
    /// A committer equal to the author is the ordinary case and every entry but
    /// one.
    fn set_identity(&self, path: &AbsPath, id: &CommitIdentity) -> Result<(), Self::Error>;

    /// Create a repository at `path`, which is not one yet.
    ///
    /// Used when the content repository is configured as `local`: the workspace
    /// is its own content repository, and on a fresh machine there is nothing
    /// there to clone from until one exists.
    fn init(&self, path: &AbsPath) -> Result<(), Self::Error>;

    /// The repository whose working tree `path` sits **inside**, if any, found
    /// by walking upward.
    ///
    /// **A path that is itself a repository is not inside one**, and reports
    /// `None`. The comparison lives here rather than in every caller because
    /// both sides have to be resolved to be compared at all: on a system where
    /// `/var` is a symlink to `/private/var`, a resolved ancestor and an
    /// unresolved subject are never equal, and a workspace refuses to stand up
    /// twice.
    ///
    /// `is_repo` only answers whether a path is itself a repository root, so a
    /// directory nested inside somebody else's checkout looks free. Initialising
    /// there produces a repository inside a repository and lands a participant's
    /// directories in a tree that is not ours.
    fn enclosing_repo(&self, path: &AbsPath) -> Result<Option<AbsPath>, Self::Error>;

    /// The URL `path`'s `origin` remote points at, if it has one.
    ///
    /// **Not where the clone URL comes from.** That is configuration, and an
    /// earlier round derived it from here on the reasoning that a configuration
    /// key would duplicate the fact. The key already existed, the derivation
    /// consulted neither, and standing up from an unrelated clone cloned that
    /// unrelated repository. This exists to *cross-check* the configured URL
    /// against what a tree actually points at.
    fn origin_url(&self, path: &AbsPath) -> Result<Option<String>, Self::Error>;

    /// The clone's configured author and committer, read back.
    ///
    /// **Four values, not two.** It returned the author alone, so a
    /// `set_identity` that wrote the committer nowhere passed the guard that
    /// exists because a write which never reached disk survived a round. The
    /// write half was widened and the read half was not.
    ///
    /// Present so setting it can be asserted rather than assumed. A write with
    /// no read is a write nobody checks.
    ///
    /// **The clone's own configuration, never the merged view.** A merged read
    /// reports the machine's global identity as though it were this
    /// repository's, which is exactly the confusion this exists to prevent.
    ///
    /// `None` when the clone configures none. An empty value is not a value.
    fn identity(&self, path: &AbsPath) -> Result<Option<CommitIdentity>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_is_refused_empty_and_the_refusal_names_which() {
        // Four separate assertions rather than one loop over a list, because a
        // loop over a hand-written list is the shape that already missed two
        // fields elsewhere in this crate. Each arm names its own part, so a
        // part that stops being checked fails here by name.
        assert_eq!(
            CommitIdentity::split("", "a@b", "c", "c@d"),
            Err(EmptyPart(Part::AuthorName))
        );
        assert_eq!(
            CommitIdentity::split("a", "", "c", "c@d"),
            Err(EmptyPart(Part::AuthorEmail))
        );
        assert_eq!(
            CommitIdentity::split("a", "a@b", "", "c@d"),
            Err(EmptyPart(Part::CommitterName))
        );
        assert_eq!(
            CommitIdentity::split("a", "a@b", "c", ""),
            Err(EmptyPart(Part::CommitterEmail))
        );
    }

    #[test]
    fn whitespace_is_empty_too() {
        // A name of three spaces configures a clone that commits as nobody just
        // as surely as one of none, and git does not trim it for us.
        for blank in [" ", "\t", "\n", "  \t \n "] {
            assert_eq!(
                CommitIdentity::split(blank, "a@b", "c", "c@d"),
                Err(EmptyPart(Part::AuthorName)),
                "{blank:?}"
            );
        }
    }

    #[test]
    fn same_puts_one_pair_in_both_roles() {
        let it = CommitIdentity::same("op", "op@example.test").unwrap();
        assert_eq!(it.author_name(), "op");
        assert_eq!(it.author_email(), "op@example.test");
        assert_eq!(it.committer_name(), "op");
        assert_eq!(it.committer_email(), "op@example.test");
    }

    #[test]
    fn same_refuses_an_empty_part_in_both_roles_at_once() {
        // The control on the delegation: `same` must not become a way past the
        // check by handing one value to two slots.
        assert_eq!(
            CommitIdentity::same("", "op@example.test"),
            Err(EmptyPart(Part::AuthorName))
        );
        assert_eq!(
            CommitIdentity::same("op", ""),
            Err(EmptyPart(Part::AuthorEmail))
        );
    }

    #[test]
    fn split_keeps_the_two_apart() {
        // The case the split exists for, and the assertion that would fail if
        // `split` quietly collapsed to `same`.
        let it =
            CommitIdentity::split("op", "op@example.test", "crew", "op+crew@example.test").unwrap();
        assert_eq!(it.author_name(), "op");
        assert_eq!(it.author_email(), "op@example.test");
        assert_eq!(it.committer_name(), "crew");
        assert_eq!(it.committer_email(), "op+crew@example.test");
        assert_ne!(it.author_email(), it.committer_email());
    }

    #[test]
    fn a_refusal_names_the_git_key_an_operator_would_go_and_look_at() {
        // The point of carrying the part rather than the fact. A message saying
        // a value was empty leaves six keys to check.
        assert_eq!(Part::AuthorName.key(), "author.name");
        assert_eq!(Part::AuthorEmail.key(), "author.email");
        assert_eq!(Part::CommitterName.key(), "committer.name");
        assert_eq!(Part::CommitterEmail.key(), "committer.email");
        assert_eq!(
            EmptyPart(Part::CommitterEmail).to_string(),
            "committer.email is empty, and an empty value is not a value"
        );
    }
}
