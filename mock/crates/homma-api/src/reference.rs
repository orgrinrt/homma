//! References, and the standing derived from them.
//!
//! A reference names something in a namespace. The sigil that introduces it says
//! which namespace, and the namespace, together with the role of what it points
//! at, is what the record's standing is derived from.
//!
//! Standing is never stored beside a reference. Storing both would let them
//! disagree, and a record claiming a standing its author does not have is the
//! failure the whole provenance discipline exists to prevent.

use std::fmt;
use std::str::FromStr;

use crate::config::Role;

/// Which namespace a reference addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// `@handle`. Someone in the registry.
    Org,
    /// `#project/slug`. A unit of work.
    Task,
    /// `&project/slug`. A planned body of work, releases included.
    Epoch,
    /// `!forge/kind/id`. Something on a forge, which we never author.
    Forge,
    /// `~label`.
    Label,
}

impl Namespace {
    /// The character that introduces a reference in this namespace.
    pub fn sigil(self) -> char {
        match self {
            Namespace::Org => '@',
            Namespace::Task => '#',
            Namespace::Epoch => '&',
            Namespace::Forge => '!',
            Namespace::Label => '~',
        }
    }

    /// The namespace a sigil introduces, if it introduces one.
    pub fn from_sigil(c: char) -> Option<Self> {
        match c {
            '@' => Some(Namespace::Org),
            '#' => Some(Namespace::Task),
            '&' => Some(Namespace::Epoch),
            '!' => Some(Namespace::Forge),
            '~' => Some(Namespace::Label),
            _ => None,
        }
    }
}

/// A reference into a namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference {
    pub namespace: Namespace,
    /// Everything after the sigil, unparsed. What it means is the namespace's
    /// business, not this type's.
    pub path:      String,
}

impl Reference {
    pub fn new(namespace: Namespace, path: impl Into<String>) -> Self {
        Self {
            namespace,
            path: path.into(),
        }
    }

    /// A reference to someone in the registry.
    pub fn org(handle: impl Into<String>) -> Self {
        Self::new(Namespace::Org, handle)
    }

    /// The standing a record authored by this reference carries.
    ///
    /// `roles` answers what role a registry handle holds. A handle the registry
    /// does not know yields [`Rung::Unknown`], which is deliberately not
    /// [`Rung::AgentOutput`]: an unrecognised author is a stronger reason for
    /// suspicion than a recognised agent, not a weaker one.
    pub fn rung(&self, roles: &dyn Fn(&str) -> Option<Role>) -> Rung {
        match self.namespace {
            Namespace::Forge => Rung::Mapped,
            Namespace::Org => {
                match roles(&self.path) {
                    Some(Role::King) => Rung::Ratified,
                    Some(Role::Hand) | Some(Role::Expert) | Some(Role::General) => {
                        Rung::AgentOutput
                    },
                    None => Rung::Unknown,
                }
            },
            // A record derived from another record inherits nothing: what wrote
            // it is what it was written by, and that is the record it points at.
            Namespace::Task | Namespace::Epoch | Namespace::Label => Rung::Derived,
        }
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.namespace.sigil(), self.path)
    }
}

/// Parsing a reference fails only one way: it did not start with a sigil.
#[derive(Debug, PartialEq, Eq)]
pub struct NotAReference;

impl fmt::Display for NotAReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not a reference: expected one of @ # & ! ~ followed by a path"
        )
    }
}

impl std::error::Error for NotAReference {}

impl FromStr for Reference {
    type Err = NotAReference;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let sigil = chars.next().ok_or(NotAReference)?;
        let namespace = Namespace::from_sigil(sigil).ok_or(NotAReference)?;
        let path = chars.as_str();
        if path.is_empty() {
            return Err(NotAReference);
        }
        Ok(Reference::new(namespace, path))
    }
}

/// The standing of a record, derived from its provenance and never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    /// A human decided it.
    Ratified,
    /// An agent produced it. Presumed wrong where it conflicts with what a human
    /// decided.
    AgentOutput,
    /// Ingested from a forge. We never author these.
    Mapped,
    /// Produced from another record.
    Derived,
    /// The author is not in the registry.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(handle: &str) -> Option<Role> {
        match handle {
            "op" => Some(Role::King),
            "paja" => Some(Role::Hand),
            "proof" => Some(Role::Expert),
            _ => None,
        }
    }

    #[test]
    fn every_sigil_round_trips() {
        for ns in [
            Namespace::Org,
            Namespace::Task,
            Namespace::Epoch,
            Namespace::Forge,
            Namespace::Label,
        ] {
            assert_eq!(Namespace::from_sigil(ns.sigil()), Some(ns));
        }
    }

    #[test]
    fn a_reference_round_trips_through_its_rendering() {
        for text in ["@paja", "#hila/reorder_scheduler", "&hila/rework", "!gh/pr/13", "~blocked"] {
            let parsed: Reference = text.parse().expect("should parse");
            assert_eq!(parsed.to_string(), text);
        }
    }

    #[test]
    fn a_bare_word_is_not_a_reference() {
        assert_eq!("paja".parse::<Reference>(), Err(NotAReference));
    }

    #[test]
    fn a_sigil_with_nothing_after_it_is_not_a_reference() {
        assert_eq!("@".parse::<Reference>(), Err(NotAReference));
    }

    #[test]
    fn the_king_is_the_only_ratified_rung() {
        assert_eq!(Reference::org("op").rung(&registry), Rung::Ratified);
        assert_eq!(Reference::org("paja").rung(&registry), Rung::AgentOutput);
        assert_eq!(Reference::org("proof").rung(&registry), Rung::AgentOutput);
    }

    #[test]
    fn an_unknown_author_is_unknown_rather_than_agent_output() {
        // Deliberate: an unrecognised handle is a stronger reason for suspicion
        // than a recognised agent, so it must not collapse into AgentOutput.
        assert_eq!(Reference::org("nobody").rung(&registry), Rung::Unknown);
    }

    #[test]
    fn a_forge_reference_is_mapped_whatever_the_registry_says() {
        let r: Reference = "!gh/pr/13".parse().unwrap();
        assert_eq!(r.rung(&registry), Rung::Mapped);
    }

    #[test]
    fn no_handle_can_mint_the_ratified_rung_by_naming_itself_op() {
        // The rung is a function of the registry, so a handle that merely looks
        // like the king's without being in the registry as king does not get it.
        let empty = |_: &str| None;
        assert_eq!(Reference::org("op").rung(&empty), Rung::Unknown);
    }
}
