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

use serde::de::IgnoredAny;
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
/// A cut write is the start of an object that stopped where the input did, so
/// the line has to begin with `{` and the bytes before the object have to fail
/// as an object left open: `Error::is_eof()`. Complete lines run together fail
/// with trailing characters instead, and text that is not an object's start
/// fails sooner, and none of those is a cut write, since reading only the last
/// object would drop what comes before it. The object is the one starting at
/// the first `{` after the line's first byte from which the rest of the line
/// is a single object. Nothing when there is none, or when the bytes before it
/// are not a cut write.
///
/// Each candidate is checked with [`IgnoredAny`], which validates and keeps
/// nothing, and only the chosen object is parsed into a [`Value`].
pub fn ending(line: &str) -> Option<Ending> {
    if !line.starts_with('{') {
        return None;
    }
    let (at, _) = line
        .match_indices('{')
        .filter(|(at, _)| *at > 0)
        .find(|(at, _)| serde_json::from_str::<IgnoredAny>(&line[*at ..]).is_ok())?;
    let before = serde_json::from_str::<IgnoredAny>(&line[.. at]).err()?;
    if !before.is_eof() {
        return None;
    }
    let object = serde_json::from_str::<Value>(&line[at ..]).ok()?;
    Some(Ending {
        object,
        dropped: at,
    })
}
