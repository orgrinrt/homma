//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A transcript line the harness left cut off.
//!
//! A write to the harness's file is sometimes left cut off inside a string, and
//! the next line is written straight after it before any newline. One line of
//! the file is then the start of an object with a complete object behind it. The
//! file does not say what made the write stop, and what the cut write held is
//! not in it, so the complete object is all there is to read, and the bytes
//! before it are dropped and counted.

use std::fmt;

use serde_json::Value;

/// The object that ends a line, and the bytes before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ending {
    pub object:  Value,
    /// How many bytes of the line come before the object. They are dropped.
    pub dropped: usize,
}

/// A line that was read as the object that ends it, as the run says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recovered {
    /// The line's number counted from 1, as the refusals count it.
    pub line:    usize,
    pub dropped: usize,
}

impl fmt::Display for Recovered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = if self.dropped == 1 { "byte" } else { "bytes" };
        write!(
            f,
            "transcript line {} does not parse whole: read the object that ends it and dropped \
             {} {bytes} before it",
            self.line, self.dropped
        )
    }
}

/// The object that ends `line`, a line that did not parse whole.
///
/// It is the one starting at the first `{` after the line's first byte from
/// which the rest of the line is a single object. Nothing when there is none,
/// which is a line that is simply not JSON, and nothing when the bytes before
/// the object are a whole value themselves: that is two complete lines run
/// together, and reading the second alone would drop a complete record.
pub fn ending(line: &str) -> Option<Ending> {
    line.match_indices('{')
        .filter(|(at, _)| *at > 0)
        .find_map(|(at, _)| {
            let object = serde_json::from_str::<Value>(&line[at ..]).ok()?;
            Some(Ending {
                object,
                dropped: at,
            })
        })
        .filter(|e| serde_json::from_str::<Value>(&line[.. e.dropped]).is_err())
}
