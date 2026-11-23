//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The constraints that span the catalogue's entries, and how an address is
//! compared.
//!
//! Each entry is checked on its own as it parses. What no single entry can
//! say is checked here: that a handle, an alias or an address is used once,
//! and that the offices name people the catalogue holds, each at most once.

use std::collections::BTreeMap;
use std::fmt;

use homma_api::{Role, Workspace};

/// An address as the catalogue compares it: trimmed, lowercased, and with a
/// plus tag stripped from its local part, so a sub-agent's tagged address is
/// its launcher's.
pub fn normalise(address: &str) -> String {
    let address = address.trim().to_lowercase();
    match address.rsplit_once('@') {
        Some((local, domain)) => {
            let local = local.split_once('+').map_or(local, |(base, _)| base);
            format!("{local}@{domain}")
        },
        None => address,
    }
}

/// Why a string cannot be a handle, or nothing when it can.
///
/// A handle becomes a directory name, a file name and a reference target, so
/// it is checked as a name: never empty, `.` or `..`, never starting with a
/// dot, and carrying no path separator and nothing but letters of any script,
/// digits, `-`, `_` and `.`.
pub fn handle_fault(handle: &str) -> Option<&'static str> {
    if handle.is_empty() {
        return Some("a handle cannot be empty");
    }
    if handle.contains('/') || handle.contains('\\') {
        return Some("a handle carrying a path separator addresses a file outside its directory");
    }
    if handle.starts_with('.') {
        return Some("a handle starting with a dot names a hidden or a parent directory");
    }
    if !handle
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Some("a handle carries only letters, digits, `-`, `_` and `.`");
    }
    None
}

/// One broken constraint of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogueFault {
    /// A handle or an alias that cannot be a name.
    BadName {
        name: String,
        why:  &'static str,
    },
    /// A handle or an alias two entries both answer to.
    NameTaken {
        name:   String,
        first:  String,
        second: String,
    },
    /// An address two entries both answer to.
    AddressShared {
        address: String,
        first:   String,
        second:  String,
    },
    /// An office whose holder the catalogue does not hold.
    HolderMissing {
        office: String,
        holder: String,
    },
    /// One person holding two offices.
    HoldsTwo {
        holder: String,
        first:  String,
        second: String,
    },
    /// An office named like a participant, which the channel could not tell
    /// apart.
    OfficeNamedLikeParticipant {
        office: String,
    },
}

impl fmt::Display for CatalogueFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogueFault::BadName {
                name,
                why,
            } => write!(f, "`{name}`: {why}"),
            CatalogueFault::NameTaken {
                name,
                first,
                second,
            } => {
                write!(
                    f,
                    "`{name}` is a handle or alias of both `[org.{first}]` and `[org.{second}]`; \
                     a name reaches one participant"
                )
            },
            CatalogueFault::AddressShared {
                address,
                first,
                second,
            } => {
                write!(
                    f,
                    "`{address}` is an address of both `[org.{first}]` and `[org.{second}]`, so a \
                     lookup by address could answer either"
                )
            },
            CatalogueFault::HolderMissing {
                office,
                holder,
            } => {
                write!(
                    f,
                    "`[offices.{office}]` names holder `{holder}`, who is not in `[org]`"
                )
            },
            CatalogueFault::HoldsTwo {
                holder,
                first,
                second,
            } => {
                write!(
                    f,
                    "`{holder}` holds both `[offices.{first}]` and `[offices.{second}]`; a person \
                     holds one office at a time"
                )
            },
            CatalogueFault::OfficeNamedLikeParticipant {
                office,
            } => {
                write!(
                    f,
                    "`[offices.{office}]` is named like a participant's handle or alias, and both \
                     are pinged by name"
                )
            },
        }
    }
}

/// Every broken constraint of the catalogue, none when it holds.
///
/// Reported whole rather than at the first, so one edit of the file fixes
/// them all.
pub fn check_catalogue(ws: &Workspace) -> Vec<CatalogueFault> {
    let mut faults = Vec::new();
    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    let mut addresses: BTreeMap<String, &str> = BTreeMap::new();
    for (key, id) in &ws.org {
        for name in std::iter::once(&id.handle).chain(&id.aliases) {
            if let Some(why) = handle_fault(name) {
                faults.push(CatalogueFault::BadName {
                    name: name.clone(),
                    why,
                });
            }
            if let Some(first) = names.insert(name, key) {
                faults.push(CatalogueFault::NameTaken {
                    name:   name.clone(),
                    first:  first.to_string(),
                    second: key.clone(),
                });
            }
        }
        let own = [&id.git_email, &id.committer_email];
        let mut seen_here = Vec::new();
        for address in own.into_iter().flatten().map(|a| normalise(a)) {
            // An entry's committer may be its author: that is one address, not two.
            if seen_here.contains(&address) {
                continue;
            }
            seen_here.push(address.clone());
            if let Some(first) = addresses.insert(address.clone(), key) {
                faults.push(CatalogueFault::AddressShared {
                    address,
                    first: first.to_string(),
                    second: key.clone(),
                });
            }
        }
    }
    let mut holders: BTreeMap<&str, &str> = BTreeMap::new();
    for (office, row) in &ws.offices {
        if names.contains_key(office.as_str()) {
            faults.push(CatalogueFault::OfficeNamedLikeParticipant {
                office: office.clone(),
            });
        }
        let Some(holder) = &row.holder else { continue };
        if !ws.org.contains_key(holder) {
            faults.push(CatalogueFault::HolderMissing {
                office: office.clone(),
                holder: holder.clone(),
            });
        }
        if let Some(first) = holders.insert(holder, office) {
            faults.push(CatalogueFault::HoldsTwo {
                holder: holder.clone(),
                first:  first.to_string(),
                second: office.clone(),
            });
        }
    }
    faults
}

/// Whether an entry in this role is an agent, whose address is matched.
pub fn is_agent(role: Role) -> bool {
    !matches!(role, Role::King)
}

#[cfg(test)]
mod tests;
