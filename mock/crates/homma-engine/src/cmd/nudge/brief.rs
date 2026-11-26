//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a brief row says of the brief made for its text.

/// What a brief row says of a brief made: the brief, nothing where nothing bears on the work, and
/// where it could not be made the reason and the command that makes it by hand.
pub(super) fn said_of(made: anyhow::Result<crate::cmd::what::brief::Brief>) -> Option<String> {
    match made {
        Ok(b) if !b.things.is_empty() => Some(b.render()),
        Ok(_) => None,
        Err(why) => {
            Some(format!(
                "The brief was not made: {why:#}. `homma what --brief -` with the work on stdin \
                 makes it by hand."
            ))
        },
    }
}
