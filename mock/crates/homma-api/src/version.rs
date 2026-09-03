//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! A semver version and how the release level moves it.

use std::fmt;
use std::str::FromStr;

use super::release::Level;

/// A semver triple with an optional prerelease, compared the way cargo and jsr
/// compare them: numerically per part, and a prerelease sorts before its own
/// release.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major:      u64,
    pub minor:      u64,
    pub patch:      u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<String>,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// The version a level makes of this one. Before 1.0 a major is a minor,
    /// since that is what every resolver reads a `0.x` bump as, and a
    /// prerelease is dropped whatever the level.
    pub fn bumped(&self, level: Level) -> Version {
        let level = if self.major == 0 && level == Level::Major { Level::Minor } else { level };
        match level {
            Level::Patch => Version::new(self.major, self.minor, self.patch + 1),
            Level::Minor => Version::new(self.major, self.minor + 1, 0),
            Level::Major => Version::new(self.major + 1, 0, 0),
        }
    }

    /// Whether `next` is exactly one legal step above this version at the
    /// given level, which is what a release refuses to skip past.
    pub fn is_smallest_successor(&self, next: &Version, level: Level) -> bool {
        &self.bumped(level) == next
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| {
                match (&self.prerelease, &other.prerelease) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => prerelease_cmp(a, b),
                }
            })
    }
}

/// Prerelease identifiers the semver way: dot-separated, a numeric one
/// compared as a number and sorting below an alphanumeric one, and a list
/// sorting below any longer list it is a prefix of.
fn prerelease_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            },
        }
    }
}

impl FromStr for Version {
    type Err = NotAVersion;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let (core, prerelease) = match s.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
            Some(_) => return Err(NotAVersion(s.to_string())),
            None => (s, None),
        };
        let mut parts = core.split('.');
        let mut part = || {
            parts
                .next()
                .and_then(|p| p.parse::<u64>().ok())
                .ok_or_else(|| NotAVersion(s.to_string()))
        };
        let (major, minor, patch) = (part()?, part()?, part()?);
        if parts.next().is_some() {
            return Err(NotAVersion(s.to_string()));
        }
        Ok(Version {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.prerelease {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

/// A string that is not `X.Y.Z` or `X.Y.Z-pre`, with or without a leading `v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAVersion(pub String);

impl fmt::Display for NotAVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a version", self.0)
    }
}

impl std::error::Error for NotAVersion {}
