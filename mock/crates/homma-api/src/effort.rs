//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The effort of a turn, in the agent host's own words.
//!
//! A fixed vocabulary: the host defines these five and reads nothing else, so a
//! word outside them is refused where it is written rather than ignored where it
//! is read.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

/// How much effort the host spends on the rest of a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    /// Every word, in rising order.
    pub const ALL: [Effort; 5] =
        [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];

    /// The host's word for it.
    pub const fn word(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

impl fmt::Display for Effort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A word that is not one of the host's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAnEffort(pub String);

impl fmt::Display for NotAnEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words: Vec<&str> = Effort::ALL.iter().map(|e| e.word()).collect();
        write!(
            f,
            "`{}` is not an effort the host reads; it reads {}",
            self.0,
            words.join(", ")
        )
    }
}

impl std::error::Error for NotAnEffort {}

impl FromStr for Effort {
    type Err = NotAnEffort;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Effort::ALL
            .into_iter()
            .find(|e| e.word() == s)
            .ok_or_else(|| NotAnEffort(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_reads_back_as_itself() {
        for e in Effort::ALL {
            assert_eq!(e.word().parse::<Effort>(), Ok(e));
        }
    }

    #[test]
    fn a_word_outside_the_five_is_refused_and_the_five_are_named() {
        let err = "extreme".parse::<Effort>().unwrap_err();
        assert!(
            err.to_string().contains("low, medium, high, xhigh, max"),
            "{err}"
        );
        assert!(
            "High".parse::<Effort>().is_err(),
            "the host's words are lowercase"
        );
    }
}
