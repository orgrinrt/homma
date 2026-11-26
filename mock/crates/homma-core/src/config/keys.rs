//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! Every key the manifest is read for, by both parsers of it.

use std::fmt;

use homma_api::{AskConfig, Keys, SearchConfig, SummaryConfig, WhatConfig, Workspace, keys_of};

use super::Config;

/// Every manifest key this engine reads, as a dotted path with a map's keys `*`.
///
/// The probe of [`Config`], with each table it holds as a raw value replaced by
/// the probe of the api type that reads that table. Derived from the types, so
/// a key added to any of them is listed with no other edit.
///
/// # Errors
///
/// [`KeysUnread`] when one of the manifest's types refuses the probe. The
/// probe reads no input, only the types, so this fails on every run alike, and
/// the tests of this function are what meet it first.
pub fn keys() -> Result<Keys, KeysUnread> {
    let mut keys = probe::<Config>()?;
    keys.graft("", probe::<Workspace>()?);
    keys.graft("search", probe::<SearchConfig>()?);
    keys.graft("summary", probe::<SummaryConfig>()?);
    keys.graft("ask", probe::<AskConfig>()?);
    keys.graft("what", probe::<WhatConfig>()?);
    Ok(keys)
}

/// A manifest type the key probe could not read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysUnread {
    pub of:  &'static str,
    pub why: String,
}

impl fmt::Display for KeysUnread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the manifest type {} refuses the key probe: {}",
            self.of, self.why
        )
    }
}

impl std::error::Error for KeysUnread {}

fn probe<T: serde::de::DeserializeOwned>() -> Result<Keys, KeysUnread> {
    keys_of::<T>().map_err(|e| {
        KeysUnread {
            of:  std::any::type_name::<T>(),
            why: e.to_string(),
        }
    })
}
