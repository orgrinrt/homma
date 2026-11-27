//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! The tables this engine no longer reads at all, which a running workspace's
//! manifest may still carry. The parse takes the manifest without them and
//! keeps their names, so they are said once rather than refused.

use homma_api::layers::MANIFEST;
use serde::Serialize;

use super::{Config, ConfigError};

/// `[hooks]`: the git hooks, declared in a `shook.toml` since; see
/// `homma_core::hooks`.
pub const HOOKS: &str = "hooks";

/// Every table this engine no longer reads.
pub const TABLES: [&str; 1] = [HOOKS];

/// A retired table that a file still carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Retired {
    /// The table's name, `hooks`.
    pub table: String,
    /// The file carrying it, `homma.toml` or an overlay's name.
    pub file:  String,
}

impl Retired {
    /// The line a status or a doctor prints for it: what is no longer read,
    /// where it moved, which is `git_hooks` under `[paths]`, and the move.
    pub fn said(&self, git_hooks: &std::path::Path) -> String {
        let to = git_hooks.display();
        format!(
            "[{}] is no longer read: the git hooks are declared in {to}. Move each row of it in {} \
             there as [hooks.<event>], `run` as an argument list and `paths` and `{{paths}}` as \
             before, then take [{}] out of {}.",
            self.table, self.file, self.table, self.file
        )
    }
}

/// Take every retired table out of `table`, naming each against `file`.
pub fn take(table: &mut toml::Table, file: &str) -> Vec<Retired> {
    TABLES
        .iter()
        .filter(|name| table.remove(**name).is_some())
        .map(|name| {
            Retired {
                table: (*name).to_string(),
                file:  file.to_string(),
            }
        })
        .collect()
}

/// Parse a manifest, taking out each retired table it carries.
///
/// The strict parse runs first, and a manifest it takes carries nothing
/// retired. Only where it refuses is the text read as a raw table, the retired
/// ones taken out and the rest parsed strictly. Where nothing was retired, the
/// first refusal is returned as it was, with its line.
pub(super) fn parse(text: &str) -> Result<Config, ConfigError> {
    let refused = match toml::from_str::<Config>(text) {
        Ok(cfg) => return Ok(cfg),
        Err(e) => e,
    };
    let Ok(mut raw) = text.parse::<toml::Table>() else {
        return Err(ConfigError::Parse(refused));
    };
    let retired = take(&mut raw, MANIFEST);
    if retired.is_empty() {
        return Err(ConfigError::Parse(refused));
    }
    let mut cfg: Config = toml::Value::Table(raw)
        .try_into()
        .map_err(ConfigError::Parse)?;
    cfg.retired = retired;
    Ok(cfg)
}
