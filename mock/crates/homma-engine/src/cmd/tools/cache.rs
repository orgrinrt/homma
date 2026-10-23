//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The directory renki's git backend fetches a tool into.
//!
//! It is the cache root the launcher keeps its own builds in, so one cleanup takes both and a tool
//! fetched once is shared by every workspace: `HOMMA_CACHE` when it is set, else the XDG cache directory
//! with `homma` under it, else the platform's default. The table is `renki-dirs`'s. What is read here is
//! the environment, since `renki` resolves the same root for its launcher half and does not hand the
//! answer to a host.

use std::ffi::OsStr;
use std::path::PathBuf;

use notko::{Maybe, Outcome};
use renki_dirs::{Cache, EnvName, Host, Kind, Namespace, Root, Short, Sources};

/// The namespace and the short name, which are the launcher's own.
const NAME: &str = "homma";

/// The cache root on this machine.
///
/// # Errors
///
/// When a variable holds something that is not text, or neither the XDG variable nor `HOME` is set.
pub fn root() -> Result<PathBuf, String> {
    let own = std::env::var_os(env_name()?);
    let xdg = std::env::var_os(Cache::XDG_VAR);
    let home = std::env::var_os("HOME");
    root_from(own.as_deref(), xdg.as_deref(), home.as_deref())
}

fn env_name() -> Result<String, String> {
    match Short::new(NAME) {
        Outcome::Ok(short) => Ok(EnvName::<Cache>::of(short).to_string()),
        Outcome::Err(e) => Err(format!("`{NAME}` is not a short name: {e:?}")),
    }
}

// FIXME: this is a copy of renki 0.0.3's `pub(crate)` `cache_root` (its `cache.rs`, `root_of` and
// `root_from`), kept because the host half of renki's extension api resolves no cache of its own. The
// upstream change that removes it is renki exposing the cache root to a host, or `extension::locate`
// taking the host's short name and namespace and resolving the cache itself. Until then a change to
// renki's precedence is a change this must follow by hand.
/// [`root`] over the three values it reads, so it is testable without setting the environment.
///
/// # Errors
///
/// As [`root`].
pub fn root_from(
    own: Option<&OsStr>,
    xdg: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, String> {
    fn text<'a>(name: &str, v: Option<&'a OsStr>) -> Result<Maybe<&'a str>, String> {
        match v {
            None => Ok(Maybe::Isnt),
            Some(s) => {
                s.to_str()
                    .map(Maybe::Is)
                    .ok_or_else(|| format!("{name} is set to something that is not text"))
            },
        }
    }
    let sources = Sources {
        own:  text(&env_name()?, own)?,
        xdg:  text(Cache::XDG_VAR, xdg)?,
        home: text("HOME", home)?,
    };
    let ns = match Namespace::new(NAME) {
        Outcome::Ok(ns) => ns,
        Outcome::Err(e) => return Err(format!("`{NAME}` is not a directory name: {e:?}")),
    };
    match Root::<Cache, Host>::resolve(ns, sources) {
        Outcome::Ok(root) => Ok(PathBuf::from(root.to_string())),
        Outcome::Err(e) => Err(e.to_string()),
    }
}
