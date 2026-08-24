//! Records, and the kinds that declare what a record is.
//!
//! Everything homma persists is a record. A message, a task, a delivery cursor,
//! an entry in a monitoring log: one shape, distinguished by what its kind says.
//!
//! Kinds are configuration rather than code. A crate that knows what a Hand is,
//! as opposed to knowing what a role is, has hardcoded one workspace's answer,
//! and homma has to run against a workspace it has never seen.

use std::collections::BTreeMap;

use crate::reference::Reference;

/// What an attribute holds.
///
/// Three, and the rest are absent deliberately. A date distinct from an instant,
/// a duration, a number, a decimal with declared precision, a recurrence, a
/// fractional rank and an ordered workflow are all designed and none is needed
/// by the records that exist. Kinds being configuration makes each additive when
/// a consumer arrives, so building them ahead of one would fix a shape nothing
/// has pushed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttrType {
    /// Free text. Whether it carries markup is the kind's business.
    Text,
    /// A point in time, as RFC 3339.
    Instant,
    /// A reference into a namespace.
    Ref,
}

/// A value in a record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Attr {
    Text(String),
    Instant(String),
    Ref(String),
}

impl Attr {
    pub fn attr_type(&self) -> AttrType {
        match self {
            Attr::Text(_) => AttrType::Text,
            Attr::Instant(_) => AttrType::Instant,
            Attr::Ref(_) => AttrType::Ref,
        }
    }
}

/// Whether records of a kind may be edited after they are written.
///
/// This is the flag that lets correspondence and structural work share a
/// vocabulary. Correspondence declares itself append-only and is never edited;
/// work declares itself mutable and changes as it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutability {
    /// Written once. A correction is another record, never an edit.
    AppendOnly,
    /// Rewritten in place as it changes.
    Mutable,
}

/// What a record of some kind carries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kind {
    pub name:       String,
    pub mutability: Mutability,
    /// Attribute name to type. Anything not named here is not part of the kind.
    #[serde(default)]
    pub attrs:      BTreeMap<String, AttrType>,
    /// Which of `attrs` a record must carry to be valid.
    #[serde(default)]
    pub required:   Vec<String>,
}

impl Kind {
    pub fn new(name: impl Into<String>, mutability: Mutability) -> Self {
        Self {
            name: name.into(),
            mutability,
            attrs: BTreeMap::new(),
            required: Vec::new(),
        }
    }

    pub fn with_attr(mut self, name: impl Into<String>, ty: AttrType) -> Self {
        self.attrs.insert(name.into(), ty);
        self
    }

    pub fn requiring(mut self, name: impl Into<String>) -> Self {
        self.required.push(name.into());
        self
    }
}

/// Anything homma persists.
///
/// Three fields are required of every record because homma's own machinery reads
/// them, and everything else is the kind's business.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// Addresses it.
    pub id:         String,
    /// Names the declaration that governs it.
    pub kind:       String,
    /// Who created it. The standing is derived from this and never stored
    /// beside it, so the two cannot disagree.
    pub provenance: String,
    #[serde(default)]
    pub attrs:      BTreeMap<String, Attr>,
}

impl Record {
    pub fn new(id: impl Into<String>, kind: impl Into<String>, provenance: &Reference) -> Self {
        Self {
            id:         id.into(),
            kind:       kind.into(),
            provenance: provenance.to_string(),
            attrs:      BTreeMap::new(),
        }
    }

    pub fn with(mut self, name: impl Into<String>, value: Attr) -> Self {
        self.attrs.insert(name.into(), value);
        self
    }

    /// Whether this record satisfies its kind: every required attribute present,
    /// every present attribute of the declared type, and nothing the kind does
    /// not declare.
    pub fn check(&self, kind: &Kind) -> Result<(), Invalid> {
        if self.kind != kind.name {
            return Err(Invalid::WrongKind {
                record:          self.kind.clone(),
                checked_against: kind.name.clone(),
            });
        }
        for name in &kind.required {
            if !self.attrs.contains_key(name) {
                return Err(Invalid::Missing(name.clone()));
            }
        }
        for (name, value) in &self.attrs {
            match kind.attrs.get(name) {
                None => return Err(Invalid::Undeclared(name.clone())),
                Some(&declared) if declared != value.attr_type() => {
                    return Err(Invalid::WrongType {
                        attr: name.clone(),
                        declared,
                        found: value.attr_type(),
                    });
                },
                Some(_) => {},
            }
        }
        Ok(())
    }
}

/// Why a record does not satisfy its kind.
#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    WrongKind {
        record:          String,
        checked_against: String,
    },
    Missing(String),
    Undeclared(String),
    WrongType {
        attr:     String,
        declared: AttrType,
        found:    AttrType,
    },
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Invalid::WrongKind {
                record,
                checked_against,
            } => {
                write!(
                    f,
                    "record is kind `{record}` but was checked against `{checked_against}`"
                )
            },
            Invalid::Missing(a) => write!(f, "required attribute `{a}` is missing"),
            Invalid::Undeclared(a) => {
                write!(f, "attribute `{a}` is not declared by this kind")
            },
            Invalid::WrongType {
                attr,
                declared,
                found,
            } => {
                write!(
                    f,
                    "attribute `{attr}` is declared {declared:?} but holds {found:?}"
                )
            },
        }
    }
}

impl std::error::Error for Invalid {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::Reference;

    fn message_kind() -> Kind {
        Kind::new("message", Mutability::AppendOnly)
            .with_attr("body", AttrType::Text)
            .with_attr("at", AttrType::Instant)
            .requiring("body")
    }

    fn rec() -> Record {
        Record::new("m1", "message", &Reference::org("paja"))
            .with("body", Attr::Text("hello".into()))
    }

    #[test]
    fn a_record_satisfying_its_kind_checks() {
        assert_eq!(rec().check(&message_kind()), Ok(()));
    }

    #[test]
    fn a_missing_required_attribute_is_refused() {
        let r = Record::new("m1", "message", &Reference::org("paja"));
        assert_eq!(
            r.check(&message_kind()),
            Err(Invalid::Missing("body".into()))
        );
    }

    #[test]
    fn an_attribute_the_kind_does_not_declare_is_refused() {
        // The kind is the whole declaration: a record cannot smuggle a field
        // past it, because a field nothing declares is a field nothing reads.
        let r = rec().with("smuggled", Attr::Text("x".into()));
        assert_eq!(
            r.check(&message_kind()),
            Err(Invalid::Undeclared("smuggled".into()))
        );
    }

    #[test]
    fn an_attribute_of_the_wrong_type_is_refused() {
        let r = rec().with("at", Attr::Text("not an instant".into()));
        assert_eq!(
            r.check(&message_kind()),
            Err(Invalid::WrongType {
                attr:     "at".into(),
                declared: AttrType::Instant,
                found:    AttrType::Text,
            })
        );
    }

    #[test]
    fn checking_against_the_wrong_kind_is_refused_rather_than_silently_passing() {
        let other = Kind::new("task", Mutability::Mutable);
        assert!(matches!(
            rec().check(&other),
            Err(Invalid::WrongKind { .. })
        ));
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let r = rec();
        let line = serde_json::to_string(&r).unwrap();
        let back: Record = serde_json::from_str(&line).unwrap();
        assert_eq!(r, back);
    }
}
