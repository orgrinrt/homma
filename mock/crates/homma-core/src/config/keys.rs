//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! Every key the manifest is read for, by both parsers of it.

use homma_api::{AskConfig, Keys, SearchConfig, SummaryConfig, Workspace, keys_of};

use super::Config;

/// Every manifest key this engine reads, as a dotted path with a map's keys `*`.
///
/// The probe of [`Config`], with each table it holds as a raw value replaced by
/// the probe of the api type that reads that table. Derived from the types, so
/// a key added to any of them is listed with no other edit.
pub fn keys() -> Keys {
    let mut keys = probe::<Config>();
    keys.graft("", probe::<Workspace>());
    keys.graft("search", probe::<SearchConfig>());
    keys.graft("summary", probe::<SummaryConfig>());
    keys.graft("ask", probe::<AskConfig>());
    keys
}

/// The probe of one of the manifest's types. It reads no input, only the type,
/// so a type that refuses it does so on every run, and the tests of [`keys`]
/// are what meet it first.
fn probe<T: serde::de::DeserializeOwned>() -> Keys {
    keys_of::<T>().unwrap_or_else(|e| {
        panic!(
            "the manifest type {} refuses the key probe: {e}",
            std::any::type_name::<T>()
        )
    })
}
