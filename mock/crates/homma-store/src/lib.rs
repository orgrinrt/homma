//! Records on disk.
//!
//! Newline-delimited records, one per line, in files namespaced by kind.
//! Appending is the common operation and the format makes it cheap.
//!
//! Whether correspondence and structural work end up sharing one physical
//! engine is not settled and belongs to the workspace's owner; two readings are
//! recorded in the design round. What is not in question is that the
//! **vocabulary** is one, so this store is written against
//! [`homma_api::Record`] and knows nothing about what a message or a task is.
//!
//! The mutability a kind declares is enforced here rather than trusted: a kind
//! that says append-only cannot have a record rewritten under it, whatever the
//! caller intended.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use homma_api::{Kind, Mutability, Record};

/// Why a store operation did not happen.
#[derive(Debug)]
pub enum Error {
    /// The kind says its records are written once.
    Immutable {
        kind: String,
        id:   String,
    },
    /// Nothing under this kind carries that id.
    NotFound {
        kind: String,
        id:   String,
    },
    /// The kind's name would address a file outside the store.
    UnsafeKind(String),
    /// Something already carries that id under this kind.
    Duplicate {
        kind: String,
        id:   String,
    },
    /// The record does not satisfy the kind it claims.
    Invalid(homma_api::record::Invalid),
    Io(std::io::Error),
    Encoding(serde_json::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Immutable {
                kind,
                id,
            } => {
                write!(
                    f,
                    "kind `{kind}` is append-only, so record `{id}` cannot be rewritten. \
                 Append a record that supersedes it instead."
                )
            },
            Error::NotFound {
                kind,
                id,
            } => write!(f, "no record `{id}` of kind `{kind}`"),
            Error::UnsafeKind(k) => {
                write!(
                    f,
                    "kind name `{k}` would address a file outside the store; a kind \
                 name carries no path separator and no parent component"
                )
            },
            Error::Duplicate {
                kind,
                id,
            } => {
                write!(
                    f,
                    "kind `{kind}` already carries a record `{id}`. Appending a second \
                 under one id lets an append-only kind be contradicted without \
                 ever rewriting anything."
                )
            },
            Error::Invalid(e) => write!(f, "{e}"),
            Error::Io(e) => write!(f, "{e}"),
            Error::Encoding(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Encoding(e)
    }
}

/// A directory of records.
///
/// One process owns a store and writes it. That is not a convention: a
/// command-line tool invoked by several agents is several processes appending to
/// one file, which is the multi-writer problem regardless of the binary they
/// share. Readers are unrestricted, and a record on disk is readable by anything.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
        }
    }

    /// Where a kind's records live. One file per kind keeps a read of one kind
    /// from paying for every other.
    fn path_for(&self, kind: &str) -> PathBuf {
        self.root.join(format!("{kind}.ndjson"))
    }

    /// A kind name is configuration, so it is attacker-reachable and is checked
    /// rather than trusted. `create_dir_all` would otherwise make traversal
    /// succeed rather than fail.
    fn check_kind_name(kind: &str) -> Result<(), Error> {
        // No component scan, because `||` short-circuits and there is nothing
        // left to scan by the time one would run: reaching past the two
        // `contains` arms means the name holds no separator, so it is one
        // component, so the only parent name it can be is the whole string.
        let bad = kind.is_empty()
            || kind.contains('/')
            || kind.contains('\\')
            || kind == ".."
            || kind.contains('\0');
        if bad {
            return Err(Error::UnsafeKind(kind.to_string()));
        }
        Ok(())
    }

    /// Add a record. The only way anything enters the store.
    pub fn append(&self, kind: &Kind, record: &Record) -> Result<(), Error> {
        Self::check_kind_name(&kind.name)?;
        record.check(kind).map_err(Error::Invalid)?;
        // Without this, a second record under an existing id lets an
        // append-only kind be contradicted without ever calling `replace`,
        // which is the only path the immutability guard covers.
        if self.read(&kind.name)?.iter().any(|r| r.id == record.id) {
            return Err(Error::Duplicate {
                kind: kind.name.clone(),
                id:   record.id.clone(),
            });
        }
        if let Some(parent) = self.path_for(&kind.name).parent() {
            fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_string(record)?;
        line.push('\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path_for(&kind.name))?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Every record of a kind, in the order it was written.
    pub fn read(&self, kind: &str) -> Result<Vec<Record>, Error> {
        // A read escapes the root exactly as a write does, and this one was
        // open: `append` validated, `read` did not, and `replace` reaches
        // `rewrite` through here. A traversing kind whose target exists outside
        // the root was therefore readable, and then rewritable.
        Self::check_kind_name(kind)?;
        let path = self.path_for(kind);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for line in BufReader::new(fs::File::open(&path)?).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&line)?);
        }
        Ok(out)
    }

    /// Replace a record in place.
    ///
    /// Refused when the kind declares itself append-only, which is the whole
    /// point of the flag: correspondence is corrected by superseding it, never
    /// by erasing what was said.
    pub fn replace(&self, kind: &Kind, record: &Record) -> Result<(), Error> {
        if kind.mutability == Mutability::AppendOnly {
            return Err(Error::Immutable {
                kind: kind.name.clone(),
                id:   record.id.clone(),
            });
        }
        record.check(kind).map_err(Error::Invalid)?;
        let mut all = self.read(&kind.name)?;
        let slot = all.iter_mut().find(|r| r.id == record.id).ok_or_else(|| {
            Error::NotFound {
                kind: kind.name.clone(),
                id:   record.id.clone(),
            }
        })?;
        *slot = record.clone();
        self.rewrite(&kind.name, &all)
    }

    fn rewrite(&self, kind: &str, all: &[Record]) -> Result<(), Error> {
        // Validated here rather than trusted from the caller, even though the
        // only caller now reaches `read` first and `read` checks too. Relying on
        // that would be a guard held in a different method, which is the shape
        // this branch's defect took fourteen times and which survives exactly
        // until someone adds a second caller.
        Self::check_kind_name(kind)?;
        let path = self.path_for(kind);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Write beside and rename, so a reader never sees a half-written file
        // and a crash leaves the previous state rather than a truncated one.
        let tmp = path.with_extension("ndjson.writing");
        {
            let mut f = fs::File::create(&tmp)?;
            for r in all {
                let mut line = serde_json::to_string(r)?;
                line.push('\n');
                f.write_all(line.as_bytes())?;
            }
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use homma_api::{Attr, AttrType, Reference};

    use super::*;

    fn message() -> Kind {
        Kind::new("message", Mutability::AppendOnly)
            .with_attr("body", AttrType::Text)
            .requiring("body")
    }

    fn task() -> Kind {
        Kind::new("task", Mutability::Mutable)
            .with_attr("title", AttrType::Text)
            .requiring("title")
    }

    fn msg(id: &str, body: &str) -> Record {
        Record::new(id, "message", &Reference::org("paja")).with("body", Attr::Text(body.into()))
    }

    fn tsk(id: &str, title: &str) -> Record {
        Record::new(id, "task", &Reference::org("vouti")).with("title", Attr::Text(title.into()))
    }

    fn store() -> (tempfile::TempDir, Store) {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path());
        (d, s)
    }

    #[test]
    fn reading_a_kind_nothing_has_written_is_empty_rather_than_an_error() {
        let (_d, s) = store();
        assert!(s.read("message").unwrap().is_empty());
    }

    #[test]
    fn read_refuses_a_traversing_kind_rather_than_opening_a_file_outside_the_root() {
        // This one was reachable through the public surface. `append` checked
        // the name and `read` did not, so a kind naming a path outside the root
        // was read and parsed from there.
        // The store is rooted one level inside the tempdir, so `..` reaches a
        // real file that is still cleaned up with the tempdir.
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path().join("root"));
        let outside = d.path().join("outside.ndjson");
        fs::write(
            &outside,
            serde_json::to_string(&tsk("t1", "not the store's")).unwrap() + "\n",
        )
        .unwrap();

        // The control: the same call against a name that does not traverse
        // reads nothing and is not an error, so a refusal below is about the
        // traversal rather than about the file being absent.
        assert!(s.read("ordinary").unwrap().is_empty());

        let err = s.read("../outside").unwrap_err();
        assert!(
            matches!(err, Error::UnsafeKind(ref k) if k == "../outside"),
            "expected the kind to be refused by name, got {err:?}"
        );
    }

    #[test]
    fn rewrite_refuses_a_traversing_kind_without_help_from_its_caller() {
        // `rewrite` is private and its only caller validates before reaching
        // it, so this cannot be provoked through the public surface today. It
        // is asserted directly because the guard's whole purpose is to hold
        // when a second caller appears, and a guard nothing names is one a
        // later edit deletes without any test going red.
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path().join("root"));
        let escaped = d.path().join("escaped.ndjson");
        assert!(
            !escaped.exists(),
            "the control: nothing is at the traversal target before the call"
        );

        let err = s.rewrite("../escaped", &[]).unwrap_err();
        assert!(
            matches!(err, Error::UnsafeKind(ref k) if k == "../escaped"),
            "expected the kind to be refused by name, got {err:?}"
        );
        assert!(
            !escaped.exists(),
            "and nothing may be written outside the store root"
        );
    }

    #[test]
    fn appended_records_read_back_in_the_order_they_were_written() {
        let (_d, s) = store();
        for (i, body) in ["first", "second", "third"].iter().enumerate() {
            s.append(&message(), &msg(&format!("m{i}"), body)).unwrap();
        }
        let got: Vec<String> = s
            .read("message")
            .unwrap()
            .iter()
            .map(|r| {
                match &r.attrs["body"] {
                    Attr::Text(t) => t.clone(),
                    _ => unreachable!(),
                }
            })
            .collect();
        assert_eq!(got, vec!["first", "second", "third"]);
    }

    #[test]
    fn an_append_only_kind_refuses_to_be_rewritten() {
        let (_d, s) = store();
        s.append(&message(), &msg("m1", "as said")).unwrap();
        let err = s
            .replace(&message(), &msg("m1", "as i wish i had said"))
            .unwrap_err();
        assert!(matches!(err, Error::Immutable { .. }));
        // And the original is untouched, which is the property that matters.
        match &s.read("message").unwrap()[0].attrs["body"] {
            Attr::Text(t) => assert_eq!(t, "as said"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_mutable_kind_accepts_a_rewrite() {
        let (_d, s) = store();
        s.append(&task(), &tsk("t1", "open it")).unwrap();
        s.replace(&task(), &tsk("t1", "opened")).unwrap();
        let all = s.read("task").unwrap();
        assert_eq!(all.len(), 1, "a rewrite replaces rather than appends");
        match &all[0].attrs["title"] {
            Attr::Text(t) => assert_eq!(t, "opened"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn rewriting_a_record_that_is_not_there_is_refused_rather_than_creating_it() {
        let (_d, s) = store();
        let err = s.replace(&task(), &tsk("ghost", "x")).unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn a_record_that_does_not_satisfy_its_kind_never_reaches_the_disk() {
        let (_d, s) = store();
        let bad = Record::new("m1", "message", &Reference::org("paja"));
        assert!(matches!(s.append(&message(), &bad), Err(Error::Invalid(_))));
        assert!(s.read("message").unwrap().is_empty());
    }

    #[test]
    fn a_kind_name_cannot_address_a_file_outside_the_store() {
        let (d, s) = store();
        let evil = Kind::new("../../escaped", Mutability::Mutable)
            .with_attr("title", AttrType::Text)
            .requiring("title");
        let r = Record::new("x", "../../escaped", &Reference::org("paja"))
            .with("title", Attr::Text("out".into()));
        assert!(matches!(s.append(&evil, &r), Err(Error::UnsafeKind(_))));
        let outside = d
            .path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("escaped.ndjson");
        assert!(!outside.exists(), "nothing may be written outside the root");
    }

    #[test]
    fn a_second_record_under_one_id_is_refused_for_both_mutabilities() {
        // Otherwise an append-only kind is contradicted by appending, without
        // ever touching the guard that refuses rewrites.
        let (_d, s) = store();
        s.append(&message(), &msg("m1", "as said")).unwrap();
        assert!(matches!(
            s.append(&message(), &msg("m1", "as i wish")),
            Err(Error::Duplicate { .. })
        ));
        let all = s.read("message").unwrap();
        assert_eq!(all.len(), 1);
        match &all[0].attrs["body"] {
            Attr::Text(t) => assert_eq!(t, "as said"),
            _ => unreachable!(),
        }

        s.append(&task(), &tsk("t1", "a")).unwrap();
        assert!(matches!(
            s.append(&task(), &tsk("t1", "b")),
            Err(Error::Duplicate { .. })
        ));
    }

    #[test]
    fn two_kinds_do_not_share_a_file() {
        let (_d, s) = store();
        s.append(&message(), &msg("m1", "hello")).unwrap();
        s.append(&task(), &tsk("t1", "do it")).unwrap();
        assert_eq!(s.read("message").unwrap().len(), 1);
        assert_eq!(s.read("task").unwrap().len(), 1);
    }

    #[test]
    fn a_rewrite_leaves_no_temporary_file_behind() {
        // The rename is what keeps a reader from seeing a half-written file.
        let (d, s) = store();
        s.append(&task(), &tsk("t1", "a")).unwrap();
        s.replace(&task(), &tsk("t1", "b")).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("writing"))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }
}
