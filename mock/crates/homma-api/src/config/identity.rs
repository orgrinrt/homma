//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What kind of participant an entry is, and whether one that could own a
//! workspace has somebody on it.

use serde::{Deserialize, Serialize};

/// What kind of participant an entry describes.
///
/// The roles are homma's, not any workspace's. What a workspace calls its
/// participants and what domains it gives them is configuration; that some are
/// people, some are standing agents, some are consulted and some are labour is
/// the structure homma itself reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The human. The only source of ratification.
    King,
    /// A standing agent with a workspace, a memory and an identity of its own.
    Hand,
    /// Dispatched for a question, never resident. Has memory, no workspace.
    Expert,
    /// Utility labour. No persona, no memory, no workspace.
    General,
    /// The lighter identity a hand's team reserves for a while: a name, an
    /// address and a few lines of temperament. No workspace, and no memory of
    /// its own.
    Journeyman,
}

impl Role {
    /// Whether a participant in this role owns a workspace on disk.
    pub fn has_workspace(self) -> bool {
        matches!(self, Role::Hand)
    }

    /// Whether a participant in this role accumulates version-controlled memory.
    pub fn has_memory(self) -> bool {
        matches!(self, Role::Hand | Role::Expert)
    }
}

/// Whether an identity that could own a workspace has somebody on it.
///
/// Named staffing rather than **standing**, because *standing* already means
/// what a reference derives, one module over, and a vocabulary crate using one
/// word for two concepts has failed at its only job.
///
/// The blind global replace that performed that rename corrupted this very
/// comment into "named staffing rather than staffing because staffing already
/// means", three words after the sentence above.
///
/// The distinction between [`Staffing::Mapped`] and [`Staffing::Incomplete`] is
/// the whole reason this is an enum rather than a list of absent fields: a mapped
/// entry lacks exactly the fields an unfinished one lacks, so any answer of the
/// shape "here is what is missing" conflates a decision with a mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staffing {
    /// The role owns no workspace at all, so there is nothing to stand up and no
    /// set of fields that would change that.
    NoWorkspace,
    /// A boundary recorded, so the same ground is not carved twice. Complete as
    /// an entry, and deliberately without a workspace.
    Mapped,
    /// Meant to have a workspace, and these fields are absent.
    Incomplete(Vec<&'static str>),
    /// Ready to be stood up.
    Staffed,
}

impl Staffing {
    /// Whether a workspace may be created for this identity.
    pub fn can_stand_up(&self) -> bool {
        matches!(self, Staffing::Staffed)
    }
}

/// One participant.
///
/// A consultant's entry carries a role and a definition and nothing else,
/// because it has no workspace, no session, no git identity and no repositories.
/// The optional fields are optional for that reason rather than for tidiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub role:            Role,
    /// What everything else addresses it by.
    pub handle:          String,
    /// Further handles that reach the same participant: a nickname beside a
    /// name, a former title. One namespace with every handle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases:         Vec<String>,
    /// Whether this entry is meant to have a workspace.
    ///
    /// Declared rather than inferred from an absent workspace. Inference is
    /// cheaper and wrong: an entry whose workspace was mistyped or dropped would
    /// read as a deliberate decision, which is the confusion this removes.
    #[serde(default)]
    pub staffed:         bool,
    /// The short name a human uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname:        Option<String>,
    /// The full form, for when the short one is too familiar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name:       Option<String>,
    /// What it is good at. A default for routing and an answer to who to ask,
    /// never a partition: work is assigned, not confined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain:          Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_name:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_email:       Option<String>,
    /// The address commits are *committed* by, when it differs from the author.
    ///
    /// **Absent means the committer is the author**, which is every ordinary
    /// entry. Present means one clone carries two identities: the author is
    /// whoever the work is attributed to, and the committer is a separate
    /// address on the same account, so signing and delivery work while the two
    /// stay distinguishable in the history.
    ///
    /// Optional rather than defaulted to the author in the type, because a
    /// default here would make "the same" and "deliberately the same"
    /// indistinguishable in the file, and the file is what a human reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_email: Option<String>,
    /// The name commits are *committed* by, when it differs from the author's.
    ///
    /// Shipped a round late. The committer is an optional name *and* email;
    /// only the email arrived, and git treats `committer.name` as a first-class
    /// key that a global one can override, so the omission was a hole rather
    /// than a simplification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_name:  Option<String>,
    /// Where its work happens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace:       Option<String>,
    /// Pinned, so restarting reaches the same participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session:         Option<String>,
    /// Added as work needs them rather than up front.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos:           Vec<String>,
}

impl Identity {
    pub fn new(role: Role, handle: impl Into<String>) -> Self {
        Self {
            role,
            handle: handle.into(),
            aliases: Vec::new(),
            staffed: false,
            nickname: None,
            full_name: None,
            domain: None,
            git_name: None,
            git_email: None,
            committer_email: None,
            committer_name: None,
            workspace: None,
            session: None,
            repos: Vec::new(),
        }
    }

    /// Whether this identity has somebody on it, and therefore whether it can
    /// be stood up.
    ///
    /// Replaces an earlier `missing()` that returned a list of absent fields.
    /// That shape cannot express the distinction that matters, because an entry
    /// which is mapped on purpose lacks exactly the fields an unfinished one
    /// lacks, so an empty-or-not answer reports a decision as a mistake.
    pub fn staffing(&self) -> Staffing {
        if !self.role.has_workspace() {
            // Not a gap that filling fields would close. Reported so a caller
            // refuses rather than building one anyway, which under the king's
            // handle would manufacture a dispatchable agent for the human.
            return Staffing::NoWorkspace;
        }
        if !self.staffed {
            return Staffing::Mapped;
        }
        let mut gaps = Vec::new();
        if self.git_name.is_none() {
            gaps.push("git_name");
        }
        if self.git_email.is_none() {
            gaps.push("git_email");
        }
        if self.workspace.is_none() {
            gaps.push("workspace");
        }
        if gaps.is_empty() { Staffing::Staffed } else { Staffing::Incomplete(gaps) }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{WITH_ORG, Workspace};
    use super::*;

    #[test]
    fn a_consultant_needs_nothing_but_a_role_and_a_handle() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        let proof = &w.org["proof"];
        assert_eq!(proof.role, Role::Expert);
        assert!(proof.workspace.is_none());
        assert!(proof.git_name.is_none());
        // Complete as an entry, and still nothing to stand up, which is a
        // different statement and is what staffing reports.
        assert_eq!(proof.staffing(), Staffing::NoWorkspace);
    }

    #[test]
    fn a_staffed_hand_missing_its_identity_says_which_fields() {
        let mut bare = Identity::new(Role::Hand, "nameless");
        bare.staffed = true;
        assert_eq!(
            bare.staffing(),
            Staffing::Incomplete(vec!["git_name", "git_email", "workspace"])
        );
    }

    #[test]
    fn a_complete_hand_is_staffed() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.org["pike"].staffing(), Staffing::Staffed);
        assert!(w.org["pike"].staffing().can_stand_up());
    }

    #[test]
    fn a_mapped_hand_is_not_reported_as_incomplete() {
        // The whole reason staffing is an enum. A mapped entry lacks exactly
        // the fields an unfinished one lacks, so a list of gaps reports a
        // decision as a mistake, and a registry of a dozen mapped entries
        // reports a dozen broken ones.
        let w = Workspace::parse(WITH_ORG).unwrap();
        let mapped = &w.org["rendering"];
        assert_eq!(mapped.staffing(), Staffing::Mapped);
        assert!(!mapped.staffing().can_stand_up());
    }

    #[test]
    fn an_entry_that_says_nothing_about_staffing_is_mapped_rather_than_broken() {
        // Through TOML, over an entry no other test touches, because the
        // property claimed is serde's default and both a constructor literal
        // and a fixture another test already asserts on would say nothing.
        let w = Workspace::parse(
            r#"
content_repo = "git@example.invalid:someone/content.git"

[org.silent]
role = "hand"
handle = "silent"
"#,
        )
        .unwrap();
        assert!(!w.org["silent"].staffed, "the default must be unstaffed");
        assert_eq!(w.org["silent"].staffing(), Staffing::Mapped);
    }

    #[test]
    fn a_role_that_owns_no_workspace_reports_that_rather_than_nothing() {
        // It reported nothing, so a caller checking for gaps found none and
        // stood one up anyway. Under the king's handle that manufactured a
        // dispatchable agent for the human.
        for role in [Role::King, Role::Expert, Role::General] {
            let id = Identity::new(role, "someone");
            assert_eq!(
                id.staffing(),
                Staffing::NoWorkspace,
                "{role:?} must report that it cannot be stood up"
            );
        }
    }

    #[test]
    fn staffing_a_role_that_owns_no_workspace_changes_nothing() {
        // Setting the flag on a King must not manufacture a path to staffing
        // one up, since the role is what decides whether a workspace exists at
        // all and the flag only says whether one is intended.
        let mut king = Identity::new(Role::King, "oz");
        king.staffed = true;
        assert_eq!(king.staffing(), Staffing::NoWorkspace);
    }
}
