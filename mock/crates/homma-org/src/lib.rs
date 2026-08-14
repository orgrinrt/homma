//! Who exists, where their work happens, and what is generated for them.
//!
//! Identity is a configuration entry, never a running process. What is lost when
//! a session ends is a transcript; what the entry describes is still true, so
//! starting it again restores a participant rather than creating a new one.

pub mod generate;
pub mod workspace;

pub use generate::{definition, write_definitions, Form, Generated};
pub use workspace::{prepare, Layout, Prepared};

use homma_api::{Identity, Role, Workspace};

/// The registry, read from a workspace's configuration.
pub struct Registry<'a> {
    workspace: &'a Workspace,
}

impl<'a> Registry<'a> {
    pub fn new(workspace: &'a Workspace) -> Self {
        Self { workspace }
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

    /// Everyone whose role owns a workspace but whose entry cannot stand one up
    /// yet, with what each is missing.
    ///
    /// This exists so the gap is reported rather than discovered halfway through
    /// a clone.
    pub fn incomplete(&self) -> Vec<(&'a Identity, Vec<&'static str>)> {
        self.workspace
            .org
            .values()
            .filter(|i| i.role.has_workspace())
            .filter_map(|i| {
                let gaps = i.missing();
                if gaps.is_empty() {
                    None
                } else {
                    Some((i, gaps))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG: &str = r#"
content_repo = "clause-dev"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
handle = "paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"

[org.nameless]
role = "hand"
handle = "nameless"

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
        let r = Registry::new(&w);
        let hands: Vec<_> = r
            .in_role(Role::Hand)
            .iter()
            .map(|i| i.handle.clone())
            .collect();
        assert_eq!(hands, vec!["nameless", "paja"]);
        assert_eq!(r.in_role(Role::King).len(), 1);
        assert_eq!(r.in_role(Role::General).len(), 0);
    }

    #[test]
    fn an_incomplete_entry_is_reported_with_what_it_lacks() {
        let w = ws();
        let r = Registry::new(&w);
        let gaps = r.incomplete();
        assert_eq!(gaps.len(), 1, "only the one hand that cannot stand up");
        assert_eq!(gaps[0].0.handle, "nameless");
        assert_eq!(gaps[0].1, vec!["git_name", "git_email", "workspace"]);
    }

    #[test]
    fn a_consultant_is_never_incomplete_because_it_owns_no_workspace() {
        let w = ws();
        let r = Registry::new(&w);
        assert!(r.incomplete().iter().all(|(i, _)| i.handle != "proof"));
    }
}
