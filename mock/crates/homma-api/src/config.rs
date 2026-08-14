//! The registry: who exists, and what each of them needs to act.
//!
//! Identity is a configuration entry, never a running process. A participant
//! outlives whatever was running it, so starting one again restores a
//! participant rather than creating a new one.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// One participant.
///
/// A consultant's entry carries a role and a definition and nothing else,
/// because it has no workspace, no session, no git identity and no repositories.
/// The optional fields are optional for that reason rather than for tidiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub role: Role,
    /// What everything else addresses it by.
    pub handle: String,
    /// The short name a human uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// The full form, for when the short one is too familiar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// What it is good at. A default for routing and an answer to who to ask,
    /// never a partition: work is assigned, not confined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_email: Option<String>,
    /// Where its work happens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Pinned, so restarting reaches the same participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Added as work needs them rather than up front.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
}

impl Identity {
    pub fn new(role: Role, handle: impl Into<String>) -> Self {
        Self {
            role,
            handle: handle.into(),
            nickname: None,
            full_name: None,
            domain: None,
            git_name: None,
            git_email: None,
            workspace: None,
            session: None,
            repos: Vec::new(),
        }
    }

    /// What is missing before this identity can be stood up.
    ///
    /// Returns the names of the fields its role requires and it does not carry.
    /// A role that owns no workspace requires none of them.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if !self.role.has_workspace() {
            // Not a gap that filling fields would close: this role has no
            // workspace to stand up at all. Reported so a caller refuses rather
            // than building one anyway, which under the king's handle would
            // manufacture a dispatchable agent for the human.
            gaps.push("a role that owns no workspace");
            return gaps;
        }
        if self.role.has_workspace() {
            if self.git_name.is_none() {
                gaps.push("git_name");
            }
            if self.git_email.is_none() {
                gaps.push("git_email");
            }
            if self.workspace.is_none() {
                gaps.push("workspace");
            }
        }
        gaps
    }
}

/// Where homma keeps things in a workspace.
///
/// Every path has a default homma will create and an override for a workspace
/// whose convention differs. Nothing about the workspace homma was born in is
/// compiled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub hands: String,
    pub experts: String,
    pub channels: String,
    pub agents: String,
    pub index: String,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            hands: ".shared/hands".into(),
            experts: ".shared/experts".into(),
            channels: ".shared/channels".into(),
            agents: ".claude/agents".into(),
            index: ".shared/.index".into(),
        }
    }
}

/// The one thing homma requires of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// The repository holding workspace metadata and content. The only required
    /// key: homma cares about neither its identity nor its contents, only that
    /// it is named.
    pub content_repo: String,
    #[serde(default)]
    pub paths: Paths,
    /// Handle to entry.
    #[serde(default)]
    pub org: BTreeMap<String, Identity>,
}

impl Workspace {
    /// The role a handle holds, for deriving a record's standing.
    pub fn role_of(&self, handle: &str) -> Option<Role> {
        self.org.get(handle).map(|i| i.role)
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Registry strings that would inject structure into anything generated
    /// from them.
    ///
    /// A generated agent definition puts these into YAML frontmatter, so a
    /// newline in a nickname writes an arbitrary key. That is how a Hand could
    /// grant its own twin the memory key the design withholds structurally, by
    /// editing its own entry in a file every participant can write.
    pub fn unsafe_strings(&self) -> Vec<(String, &'static str)> {
        let mut bad = Vec::new();
        for (handle, id) in &self.org {
            for (field, value) in [
                ("handle", Some(&id.handle)),
                ("nickname", id.nickname.as_ref()),
                ("full_name", id.full_name.as_ref()),
                ("domain", id.domain.as_ref()),
                ("git_name", id.git_name.as_ref()),
                ("git_email", id.git_email.as_ref()),
            ] {
                if let Some(v) = value {
                    if v.chars().any(|c| c.is_control()) {
                        bad.push((handle.clone(), field));
                    }
                }
            }
        }
        bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
content_repo = "clause-dev"
"#;

    const WITH_ORG: &str = r#"
content_repo = "clause-dev"

[paths]
hands = "custom/hands"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
handle = "paja"
nickname = "Paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"
repos = ["homma"]

[org.proof]
role = "expert"
handle = "proof"
"#;

    #[test]
    fn one_key_is_enough() {
        let w = Workspace::parse(MINIMAL).expect("should parse");
        assert_eq!(w.content_repo, "clause-dev");
        assert!(w.org.is_empty());
    }

    #[test]
    fn every_path_defaults_and_creating_none_of_them_is_valid() {
        let w = Workspace::parse(MINIMAL).unwrap();
        assert_eq!(w.paths.hands, ".shared/hands");
        assert_eq!(w.paths.channels, ".shared/channels");
    }

    #[test]
    fn an_override_replaces_only_what_it_names() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.paths.hands, "custom/hands");
        // The others keep their defaults rather than vanishing.
        assert_eq!(w.paths.experts, ".shared/experts");
    }

    #[test]
    fn a_consultant_needs_nothing_but_a_role_and_a_handle() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        let proof = &w.org["proof"];
        assert_eq!(proof.role, Role::Expert);
        assert!(proof.workspace.is_none());
        assert!(proof.git_name.is_none());
        // It is complete as an entry, and there is still nothing to stand up,
        // which is a different statement and is what `missing` now reports.
        assert_eq!(proof.missing(), vec!["a role that owns no workspace"]);
    }

    #[test]
    fn a_hand_missing_its_identity_says_which_fields() {
        let bare = Identity::new(Role::Hand, "nameless");
        assert_eq!(bare.missing(), vec!["git_name", "git_email", "workspace"]);
    }

    #[test]
    fn a_complete_hand_is_missing_nothing() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert!(w.org["paja"].missing().is_empty());
    }

    #[test]
    fn a_role_that_owns_no_workspace_reports_that_rather_than_nothing() {
        // It reported nothing, so a caller checking for gaps found none and
        // stood one up anyway. Under the king's handle that manufactured a
        // dispatchable agent for the human.
        for role in [Role::King, Role::Expert, Role::General] {
            let id = Identity::new(role, "someone");
            assert_eq!(
                id.missing(),
                vec!["a role that owns no workspace"],
                "{role:?} must report that it cannot be stood up"
            );
        }
    }

    #[test]
    fn a_control_character_in_any_registry_string_is_reported() {
        // A newline here becomes an arbitrary key in generated frontmatter.
        let mut w = Workspace::parse(MINIMAL).unwrap();
        let mut id = Identity::new(Role::Hand, "paja");
        id.nickname = Some("Paja\nmemory: project".into());
        w.org.insert("paja".into(), id);
        let bad = w.unsafe_strings();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0], ("paja".to_string(), "nickname"));
    }

    #[test]
    fn a_clean_registry_reports_nothing_unsafe() {
        assert!(Workspace::parse(WITH_ORG)
            .unwrap()
            .unsafe_strings()
            .is_empty());
    }

    #[test]
    fn the_registry_answers_what_role_a_handle_holds() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.role_of("op"), Some(Role::King));
        assert_eq!(w.role_of("paja"), Some(Role::Hand));
        assert_eq!(w.role_of("stranger"), None);
    }
}
