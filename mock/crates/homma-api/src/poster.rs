//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! What a poster leaves behind when it gives up. A poster is the detached
//! process a pre-push hook leaves to post a gate run's status once the forge
//! knows the commit, and it runs with no streams, so the store is the one
//! place it can say that the forge never did.

use std::fmt;

use crate::record::{Attr, AttrType, Kind, Mutability, Record};
use crate::reference::{Namespace, Reference};

/// The kind a give-up is stored under.
pub const POSTER_GAVE_UP_KIND: &str = "poster-gave-up";

/// A poster that asked after a commit for its whole bound and never posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosterGaveUp {
    /// The repository's name in the workspace configuration.
    pub repo:        String,
    /// The full sha the run it would have posted measured.
    pub sha:         String,
    /// RFC 3339, when it stopped asking.
    pub gave_up_at:  String,
    /// How long it asked for, in whole seconds, as text because the store
    /// has no number type.
    pub waited_secs: u64,
}

impl PosterGaveUp {
    /// The kind, with the four attributes every give-up carries.
    pub fn kind() -> Kind {
        Kind::new(POSTER_GAVE_UP_KIND, Mutability::AppendOnly)
            .with_attr("repo", AttrType::Text)
            .with_attr("sha", AttrType::Text)
            .with_attr("gave_up_at", AttrType::Instant)
            .with_attr("waited_secs", AttrType::Text)
            .requiring("repo")
            .requiring("sha")
            .requiring("gave_up_at")
            .requiring("waited_secs")
    }

    /// The record this give-up is stored as. The id is the repo, the sha and
    /// the time, so two posters giving up on one commit are two records.
    pub fn to_record(&self) -> Record {
        let provenance = Reference::new(Namespace::Forge, self.repo.clone());
        Record::new(
            format!("{}:{}:{}", self.repo, self.sha, self.gave_up_at),
            POSTER_GAVE_UP_KIND,
            &provenance,
        )
        .with("repo", Attr::Text(self.repo.clone()))
        .with("sha", Attr::Text(self.sha.clone()))
        .with("gave_up_at", Attr::Instant(self.gave_up_at.clone()))
        .with("waited_secs", Attr::Text(self.waited_secs.to_string()))
    }

    /// The give-up a record holds, or why it does not.
    pub fn from_record(record: &Record) -> Result<PosterGaveUp, NotAGiveUp> {
        record
            .check(&Self::kind())
            .map_err(|e| NotAGiveUp(format!("{e:?}")))?;
        let text = |name: &str| {
            match record.attrs.get(name) {
                Some(Attr::Text(s)) | Some(Attr::Instant(s)) => Ok(s.clone()),
                _ => Err(NotAGiveUp(format!("`{name}` missing"))),
            }
        };
        let waited = text("waited_secs")?;
        let waited_secs = waited
            .parse()
            .map_err(|_| NotAGiveUp(format!("waited_secs `{waited}` is not a count")))?;
        Ok(PosterGaveUp {
            repo: text("repo")?,
            sha: text("sha")?,
            gave_up_at: text("gave_up_at")?,
            waited_secs,
        })
    }
}

/// A record that is not a poster's give-up, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAGiveUp(pub String);

impl fmt::Display for NotAGiveUp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a poster give-up: {}", self.0)
    }
}

impl std::error::Error for NotAGiveUp {}

#[cfg(test)]
mod tests {
    use super::*;

    fn one() -> PosterGaveUp {
        PosterGaveUp {
            repo:        "notko".into(),
            sha:         "0123456789abcdef0123456789abcdef01234567".into(),
            gave_up_at:  "2026-09-03T21:00:00Z".into(),
            waited_secs: 600,
        }
    }

    #[test]
    fn a_give_up_survives_the_store_and_the_count_comes_back_as_a_number() {
        let record = one().to_record();
        assert_eq!(record.kind, POSTER_GAVE_UP_KIND);
        assert_eq!(PosterGaveUp::from_record(&record).unwrap(), one());
    }

    #[test]
    fn a_gate_run_record_is_not_a_give_up_and_a_bad_count_is_refused() {
        let run = crate::GateRun {
            repo:    "notko".into(),
            sha:     one().sha,
            ran_at:  one().gave_up_at,
            verdict: crate::Verdict::Green,
            steps:   Vec::new(),
        };
        assert!(PosterGaveUp::from_record(&run.to_record()).is_err());
        let bad = one()
            .to_record()
            .with("waited_secs", Attr::Text("soon".into()));
        assert!(matches!(
            PosterGaveUp::from_record(&bad),
            Err(NotAGiveUp(why)) if why.contains("soon")
        ));
    }

    #[test]
    fn two_give_ups_on_one_commit_are_two_records() {
        let later = PosterGaveUp {
            gave_up_at: "2026-09-03T21:10:00Z".into(),
            ..one()
        };
        assert_ne!(one().to_record().id, later.to_record().id);
    }
}
