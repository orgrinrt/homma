//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which tool results are an ask round, and the ones whose question is missing.

use std::fmt;

/// A tool result whose call no earlier line holds, and whether it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unasked {
    /// The line's number counted from 1, as the refusals count it.
    pub line: usize,
    /// Whether its own shape let it be read as a round.
    pub read: bool,
}

impl fmt::Display for Unasked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "")
    }
}
