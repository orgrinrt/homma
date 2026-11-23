//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The identity git writes on an internal tree's commits: the author the agent
//! that made the change, the committer the coordinator responsible for it, a
//! sub-agent its launcher with a plus tag on the address.

use std::fmt;

use homma_api::{CommitIdentity, Identity, Workspace};

use crate::catalogue::{is_agent, resolve};

/// The longest tag a sub-agent's address carries.
pub const TAG_MAX: usize = 40;

/// An identity as a git configuration file, `[author]` and `[committer]`, for an
/// `includeIf` to name.
pub fn git_config(id: &CommitIdentity) -> String {
    format!(
        "[author]\n\tname = {}\n\temail = {}\n[committer]\n\tname = {}\n\temail = {}\n",
        quote(id.author_name()),
        id.author_email(),
        quote(id.committer_name()),
        id.committer_email()
    )
}

/// A value git's configuration reads back as written: quoted, with a quote and
/// a backslash escaped.
fn quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Why no identity could be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitIdentityRefusal {
    /// No handle or alias of the catalogue is this name.
    Unknown(String),
    /// The name is the king's, whose own identity is git's global
    /// configuration and never an agent's.
    King(String),
    /// The entry has no `git_name`, or no `git_email` with an `@` in it.
    Unaddressed(String),
    /// The tag is not one to `TAG_MAX` of lowercase letters, digits and hyphens.
    BadTag(String),
}

impl fmt::Display for GitIdentityRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(n) => write!(f, "`{n}` is no handle or alias of the catalogue"),
            Self::King(n) => {
                write!(
                    f,
                    "`{n}` is the king, a person whose identity is git's global configuration and never an agent's"
                )
            },
            Self::Unaddressed(n) => {
                write!(
                    f,
                    "`{n}` has no `git_name`, or no `git_email` with an `@`, so there is no identity to write"
                )
            },
            Self::BadTag(t) => {
                write!(
                    f,
                    "the tag `{t}` is not one to {TAG_MAX} of lowercase letters, digits and hyphens"
                )
            },
        }
    }
}

impl std::error::Error for GitIdentityRefusal {}

/// The identity an internal tree's commits carry: `author` and `committer` are
/// handles or aliases of the catalogue, the committer the author where none is
/// given, and `tag` names a sub-agent on the author's address.
pub fn git_identity(
    ws: &Workspace,
    author: &str,
    committer: Option<&str>,
    tag: Option<&str>,
) -> Result<CommitIdentity, GitIdentityRefusal> {
    if let Some(t) = tag {
        let ok = !t.is_empty()
            && t.chars().count() <= TAG_MAX
            && t.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !ok {
            return Err(GitIdentityRefusal::BadTag(t.to_string()));
        }
    }
    let author_entry = agent(ws, author)?;
    let committer_entry = match committer {
        Some(c) => agent(ws, c)?,
        None => author_entry,
    };
    let (author_name, mut author_email) = signature(author_entry, author)?;
    if let Some(t) = tag {
        let at = author_email
            .find('@')
            .expect("signature refuses an address with no @");
        author_email = format!("{}+{t}{}", &author_email[.. at], &author_email[at ..]);
    }
    let (committer_name, committer_email) =
        signature(committer_entry, committer.unwrap_or(author))?;
    CommitIdentity::split(author_name, author_email, committer_name, committer_email)
        .map_err(|_| GitIdentityRefusal::Unaddressed(author.to_string()))
}

fn agent<'a>(ws: &'a Workspace, name: &str) -> Result<&'a Identity, GitIdentityRefusal> {
    let id = resolve(ws, name).ok_or_else(|| GitIdentityRefusal::Unknown(name.to_string()))?;
    if !is_agent(id.role) {
        return Err(GitIdentityRefusal::King(name.to_string()));
    }
    Ok(id)
}

/// The name and the address git writes for an entry.
fn signature(id: &Identity, name: &str) -> Result<(String, String), GitIdentityRefusal> {
    let unaddressed = || GitIdentityRefusal::Unaddressed(name.to_string());
    let git_name = id.git_name.as_deref().filter(|n| !n.trim().is_empty());
    let email = id
        .git_email
        .as_deref()
        .filter(|e| e.contains('@') && !e.starts_with('@'));
    match (git_name, email) {
        (Some(n), Some(e)) => Ok((n.to_string(), e.to_string())),
        _ => Err(unaddressed()),
    }
}

#[cfg(test)]
mod tests;
