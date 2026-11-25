//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Who exists, where their work happens, and what is generated for them.
//!
//! Identity is a configuration entry, never a running process. What is lost when
//! a session ends is a transcript; what the entry describes is still true, so
//! starting it again restores a participant rather than creating a new one.

pub mod agents;
pub mod catalogue;
pub mod character;
pub mod configs;
pub mod generate;
pub mod lease;
pub mod modes;
pub mod provision;
pub mod remote;
pub mod rules;
pub mod skills;
pub mod surface;
pub mod workspace;
mod yaml;

pub use catalogue::{CatalogueFault, check_catalogue};
pub use generate::{Form, Generated, definition, definition_described, write_definitions};
use homma_api::{Identity, Role, Staffing, Workspace};
pub use provision::{ProvisionError, Provisioned, provision};
pub use remote::{repo_name, same_repo};
pub use workspace::{Layout, Prepared, prepare};

/// The registry, read from a workspace's configuration.
pub struct OrgRegistry<'a> {
    workspace: &'a Workspace,
}

impl<'a> OrgRegistry<'a> {
    pub fn new(workspace: &'a Workspace) -> Self {
        Self {
            workspace,
        }
    }

    pub fn get(&self, handle: &str) -> Option<&'a Identity> {
        self.workspace.org.get(handle)
    }

    /// Everyone in a role, in a stable order.
    ///
    /// Stable because the underlying map is ordered: a listing that reshuffles
    /// between runs is one nobody can diff.
    pub fn in_role(&self, role: Role) -> Vec<&'a Identity> {
        self.workspace
            .org
            .values()
            .filter(|i| i.role == role)
            .collect()
    }

    pub fn all(&self) -> Vec<&'a Identity> {
        self.workspace.org.values().collect()
    }

    /// Everyone meant to have a workspace whose entry cannot yet stand one up,
    /// with what each is missing.
    ///
    /// This exists so the gap is reported rather than discovered halfway through
    /// a clone. **A mapped identity is not incomplete** and does not appear
    /// here: it has no workspace on purpose, and reporting it would make a
    /// roster of mapped domains look like a roster of mistakes.
    pub fn incomplete(&self) -> Vec<(&'a Identity, Vec<&'static str>)> {
        self.workspace
            .org
            .values()
            .filter_map(|i| {
                match i.staffing() {
                    Staffing::Incomplete(gaps) => Some((i, gaps)),
                    _ => None,
                }
            })
            .collect()
    }

    /// The agent an address belongs to, compared as [`catalogue::normalise`]
    /// compares, so a sub-agent's plus-tagged address is its launcher's. The
    /// king is never answered: an address is an agent's or it is not.
    pub fn agent_at(&self, address: &str) -> Option<&'a Identity> {
        let wanted = catalogue::normalise(address);
        self.agent_addresses()
            .into_iter()
            .find(|(a, _)| *a == wanted)
            .map(|(_, id)| id)
    }

    /// Every agent address with its entry, normalised, in the catalogue's
    /// order: the author's, then the committer's where it differs.
    pub fn agent_addresses(&self) -> Vec<(String, &'a Identity)> {
        let mut out: Vec<(String, &'a Identity)> = Vec::new();
        for id in self
            .workspace
            .org
            .values()
            .filter(|i| catalogue::is_agent(i.role))
        {
            for address in [&id.git_email, &id.committer_email].into_iter().flatten() {
                let address = catalogue::normalise(address);
                if !out
                    .iter()
                    .any(|(a, i)| *a == address && i.handle == id.handle)
                {
                    out.push((address, id));
                }
            }
        }
        out
    }

    /// Everyone holding a domain without a workspace, which is a decision
    /// recorded rather than work half done.
    pub fn mapped(&self) -> Vec<&'a Identity> {
        self.workspace
            .org
            .values()
            .filter(|i| i.staffing() == Staffing::Mapped)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG: &str = r#"
content_repo = "git@example.invalid:someone/content.git"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
staffed = true
handle = "paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"

[org.nameless]
role = "hand"
staffed = true
handle = "nameless"

[org.rendering]
role = "hand"
handle = "rendering"
domain = "rendering"

[org.proof]
role = "expert"
handle = "proof"
"#;

    fn ws() -> Workspace {
        Workspace::parse(ORG).unwrap()
    }

    #[test]
    fn a_role_lists_only_its_own() {
        let w = ws();
        let r = OrgRegistry::new(&w);
        let hands: Vec<_> = r
            .in_role(Role::Hand)
            .iter()
            .map(|i| i.handle.clone())
            .collect();
        assert_eq!(hands, vec!["nameless", "paja", "rendering"]);
        assert_eq!(r.in_role(Role::King).len(), 1);
        assert_eq!(r.in_role(Role::General).len(), 0);
    }

    #[test]
    fn an_incomplete_entry_is_reported_with_what_it_lacks() {
        let w = ws();
        let r = OrgRegistry::new(&w);
        let gaps = r.incomplete();
        assert_eq!(gaps.len(), 1, "only the one hand that cannot stand up");
        assert_eq!(gaps[0].0.handle, "nameless");
        assert_eq!(gaps[0].1, vec!["git_name", "git_email", "workspace"]);
    }

    #[test]
    fn a_mapped_entry_is_not_reported_as_incomplete() {
        // It lacks exactly the fields `nameless` lacks. Reporting both the same
        // way turns a roster of drawn boundaries into a roster of mistakes, and
        // a report that is wrong once per mapped domain is one nobody reads.
        let w = ws();
        let r = OrgRegistry::new(&w);
        assert!(
            r.incomplete().iter().all(|(i, _)| i.handle != "rendering"),
            "a domain mapped on purpose is not a gap"
        );
        assert_eq!(
            r.mapped().iter().map(|i| &i.handle).collect::<Vec<_>>(),
            vec!["rendering"]
        );
    }

    #[test]
    fn an_address_is_matched_to_its_agent_through_case_and_a_plus_tag() {
        let w = ws();
        let r = OrgRegistry::new(&w);
        assert_eq!(
            r.agent_at("paja@example.invalid")
                .map(|i| i.handle.as_str()),
            Some("paja")
        );
        assert_eq!(
            r.agent_at("Paja+review-pass@EXAMPLE.invalid")
                .map(|i| i.handle.as_str()),
            Some("paja"),
            "a sub-agent commits as its launcher's address with a tag"
        );
        assert_eq!(
            r.agent_at("someone@example.invalid"),
            None,
            "a human is no agent"
        );
        assert_eq!(
            r.agent_at("pajax@example.invalid"),
            None,
            "an exact match, not a prefix"
        );
    }

    #[test]
    fn the_king_is_never_matched_as_an_agent() {
        let text = ORG.replace(
            "role = \"king\"\nhandle = \"op\"",
            "role = \"king\"\nhandle = \"op\"\ngit_email = \"op@example.invalid\"",
        );
        let w = Workspace::parse(&text).unwrap();
        let r = OrgRegistry::new(&w);
        assert_eq!(r.agent_at("op@example.invalid"), None);
        assert!(r.agent_addresses().iter().all(|(_, i)| i.handle != "op"));
    }

    #[test]
    fn a_committer_address_is_an_agent_address_too_listed_once_when_it_repeats() {
        let both = |committer: &str| {
            let text = ORG.replace(
                "git_email = \"paja@example.invalid\"",
                &format!("git_email = \"paja@example.invalid\"\ncommitter_email = \"{committer}\""),
            );
            Workspace::parse(&text).unwrap()
        };
        let addresses = |w: &Workspace| -> Vec<String> {
            OrgRegistry::new(w)
                .agent_addresses()
                .into_iter()
                .filter(|(_, i)| i.handle == "paja")
                .map(|(a, _)| a)
                .collect()
        };
        // A distinct committer address is listed and matched.
        let w = both("paja-commits@example.invalid");
        assert_eq!(addresses(&w), vec![
            "paja@example.invalid",
            "paja-commits@example.invalid"
        ]);
        assert_eq!(
            OrgRegistry::new(&w)
                .agent_at("Paja-Commits@example.invalid")
                .map(|i| i.handle.as_str()),
            Some("paja")
        );
        // One that normalises to the author's is the same address, listed once.
        assert_eq!(addresses(&both("paja+c@example.invalid")), vec![
            "paja@example.invalid"
        ]);
    }

    #[test]
    fn a_consultant_is_never_incomplete_because_it_owns_no_workspace() {
        let w = ws();
        let r = OrgRegistry::new(&w);
        assert!(r.incomplete().iter().all(|(i, _)| i.handle != "proof"));
        // Nor mapped, which is a statement about a workspace it could have had.
        assert!(r.mapped().iter().all(|i| i.handle != "proof"));
    }
}
