//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Who holds an identity now: the lease record and the rule over it.
//!
//! An identity is held by one session at a time, so it acts as one person in
//! one place. The record is a file per identity on the workspace trunk, and
//! its history is the ledger of who held a name when. This module is the
//! rule alone, pure over the record there is, what is asked and the time;
//! writing the answer is the swap's.

use std::fmt;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

/// The record of one identity held by one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub handle:   String,
    /// The session holding it.
    pub session:  String,
    /// What it holds it for: a role, a job, a track.
    pub job:      String,
    /// When this session took it.
    pub taken:    Timestamp,
    /// When it lapses unless renewed.
    pub expires:  Timestamp,
    /// The session this one took it over from, where it was taken over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
}

impl Lease {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The record as its file holds it.
    pub fn render(&self) -> String {
        toml::to_string(self).expect("a lease is strings and timestamps, which TOML holds")
    }

    /// Whether it still holds at `now`.
    pub fn is_live(&self, now: Timestamp) -> bool {
        now < self.expires
    }
}

/// What a session asks of an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask<'a> {
    /// Hold it for `job`. A live lease of another session is taken over only
    /// when `replaces` names that very session.
    Take {
        session:  &'a str,
        job:      &'a str,
        replaces: Option<&'a str>,
    },
    /// Hold it longer.
    Renew {
        session: &'a str,
    },
    /// Let it go.
    Release {
        session: &'a str,
    },
}

/// What the answer asks the swap to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Write this record.
    Write(Lease),
    /// Remove the record: the identity is free.
    Remove,
    /// Nothing changes.
    Nothing,
}

/// Why an ask is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Another session holds it live.
    Held {
        holder:  String,
        job:     String,
        expires: Timestamp,
    },
    /// A takeover named a session that is not the holder.
    ReplacesAnother {
        named:  String,
        holder: String,
    },
    /// A renewal or release by a session that does not hold it; `holder` is
    /// whoever does, none when it is free.
    NotHolder {
        holder: Option<String>,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Held {
                holder,
                job,
                expires,
            } => {
                write!(
                    f,
                    "held by session `{holder}` for `{job}` until {expires}; a takeover names it \
                     with `--replaces {holder}`"
                )
            },
            Refusal::ReplacesAnother {
                named,
                holder,
            } => {
                write!(
                    f,
                    "the takeover names `{named}`, and the holder is `{holder}`"
                )
            },
            Refusal::NotHolder {
                holder: Some(holder),
            } => write!(f, "held by session `{holder}`, not by this one"),
            Refusal::NotHolder {
                holder: None,
            } => write!(f, "nobody holds it; take it first"),
        }
    }
}

impl std::error::Error for Refusal {}

/// The answer to `ask` of `handle`, given the record there is now.
///
/// `minutes` is how long a written lease lasts from `now`.
pub fn decide(
    current: Option<&Lease>,
    handle: &str,
    ask: Ask<'_>,
    now: Timestamp,
    minutes: u32,
) -> Result<Outcome, Refusal> {
    let expires = now + SignedDuration::from_mins(i64::from(minutes));
    match ask {
        Ask::Take {
            session,
            job,
            replaces,
        } => {
            let Some(held) = current else {
                return Ok(Outcome::Write(Lease {
                    handle: handle.to_string(),
                    session: session.to_string(),
                    job: job.to_string(),
                    taken: now,
                    expires,
                    replaces: None,
                }));
            };
            if held.session == session {
                return Ok(Outcome::Write(Lease {
                    job: job.to_string(),
                    expires,
                    ..held.clone()
                }));
            }
            if let Some(named) = replaces.filter(|n| *n != held.session) {
                return Err(Refusal::ReplacesAnother {
                    named:  named.to_string(),
                    holder: held.session.clone(),
                });
            }
            if held.is_live(now) && replaces.is_none() {
                return Err(Refusal::Held {
                    holder:  held.session.clone(),
                    job:     held.job.clone(),
                    expires: held.expires,
                });
            }
            Ok(Outcome::Write(Lease {
                handle: handle.to_string(),
                session: session.to_string(),
                job: job.to_string(),
                taken: now,
                expires,
                replaces: Some(held.session.clone()),
            }))
        },
        Ask::Renew {
            session,
        } => {
            match current {
                Some(held) if held.session == session => {
                    Ok(Outcome::Write(Lease {
                        expires,
                        ..held.clone()
                    }))
                },
                other => {
                    Err(Refusal::NotHolder {
                        holder: other.map(|h| h.session.clone()),
                    })
                },
            }
        },
        Ask::Release {
            session,
        } => {
            match current {
                None => Ok(Outcome::Nothing),
                Some(held) if held.session == session => Ok(Outcome::Remove),
                Some(held) => {
                    Err(Refusal::NotHolder {
                        holder: Some(held.session.clone()),
                    })
                },
            }
        },
    }
}

#[cfg(test)]
mod tests;
