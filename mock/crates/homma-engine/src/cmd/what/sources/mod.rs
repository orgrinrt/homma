//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where the things are read from. Each source returns what it read and what it could not.

pub mod corpora;
pub mod mock;
pub mod scripts;
#[cfg(unix)]
pub mod tools;
pub mod verbs;
