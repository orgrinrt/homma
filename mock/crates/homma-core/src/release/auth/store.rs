//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! Where the publish tokens are held. On macOS the keychain, one generic
//! password per registry with the expiry in its comment; on any other
//! system the store homma already reads secrets from, the variable a
//! registry's `token_env` names, which can be read and not written.

use std::fmt;
use std::path::PathBuf;

use crate::release::day::Day;
use crate::release::sh;

/// A secret, and the day it stops being trustworthy where one is recorded.
#[derive(Clone, PartialEq, Eq)]
pub struct Stored {
    pub secret:  String,
    pub expires: Option<Day>,
}

/// Everything but the secret, which never reaches a log or a panic message.
impl fmt::Debug for Stored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stored")
            .field("secret", &"<redacted>")
            .field("expires", &self.expires.map(|d| d.to_string()))
            .finish()
    }
}

impl Stored {
    /// Days from `today` to the expiry, negative once it passed.
    pub fn days_left(&self, today: Day) -> Option<i64> {
        self.expires.map(|e| today.until(e))
    }
}

/// The store cannot do what was asked, and why, in words for the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unavailable {}

/// Get, put and forget a secret by registry name.
pub trait Store {
    /// What the store is, for "no token in ...".
    fn name(&self) -> String;
    /// The stored secret, `None` where nothing is stored under the name.
    fn get(&self, key: &str) -> Result<Option<Stored>, Unavailable>;
    /// Store, replacing whatever was there.
    fn put(&self, key: &str, secret: &str, expires: Option<Day>) -> Result<(), Unavailable>;
    /// Delete; `false` where there was nothing to delete.
    fn forget(&self, key: &str) -> Result<bool, Unavailable>;
    /// Whether a write under `key` could land, before anyone is asked for a secret to write.
    fn writable(&self, _key: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

/// The keychain item's service prefix, one service per registry so a search
/// finds them together.
pub const SERVICE_PREFIX: &str = "hiisi-release";

/// The keychain item's account.
pub const ACCOUNT: &str = "publish";

/// The macOS keychain through its `security` command, one generic password
/// per registry. The program is named so a test can name one that is not
/// there, which is the case that crashed the tool this replaces.
///
/// `security` takes the secret as an argument, readable in the process list
/// for the moment the call runs; it has no stdin form. No `-T` is passed:
/// an empty trusted-application list makes every read raise a prompt a
/// script cannot answer.
pub struct Keychain {
    pub program: String,
    pub account: String,
}

impl Keychain {
    pub fn new() -> Self {
        Self {
            program: "security".into(),
            account: ACCOUNT.into(),
        }
    }

    fn service(key: &str) -> String {
        format!("{SERVICE_PREFIX}-{key}")
    }

    fn run(&self, args: &[&str]) -> Result<sh::Output, Unavailable> {
        sh::run(&PathBuf::from("."), &self.program, args).map_err(|e| {
            Unavailable(format!(
                "the keychain cannot be read: `{}` did not start ({})",
                self.program, e.source
            ))
        })
    }
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for Keychain {
    fn name(&self) -> String {
        "the keychain".into()
    }

    fn get(&self, key: &str) -> Result<Option<Stored>, Unavailable> {
        let svc = Self::service(key);
        let got = self.run(&["find-generic-password", "-a", &self.account, "-s", &svc, "-w"])?;
        let secret = got.stdout.trim().to_string();
        if !got.ok() || secret.is_empty() {
            return Ok(None);
        }
        let shown = self.run(&["find-generic-password", "-a", &self.account, "-s", &svc])?;
        Ok(Some(Stored {
            secret,
            expires: comment_date(&shown.stdout),
        }))
    }

    fn put(&self, key: &str, secret: &str, expires: Option<Day>) -> Result<(), Unavailable> {
        let svc = Self::service(key);
        let when = expires.map(|d| d.to_string());
        let mut args =
            vec!["add-generic-password", "-a", &self.account, "-s", &svc, "-w", secret, "-U"];
        if let Some(w) = &when {
            args.extend(["-j", w.as_str()]);
        }
        let done = self.run(&args)?;
        if done.ok() {
            Ok(())
        } else {
            Err(Unavailable(format!(
                "the keychain refused to store {key}: {}",
                done.stderr.trim()
            )))
        }
    }

    fn forget(&self, key: &str) -> Result<bool, Unavailable> {
        let svc = Self::service(key);
        Ok(self
            .run(&["delete-generic-password", "-a", &self.account, "-s", &svc])?
            .ok())
    }
}

/// The expiry `security` prints in an item's comment line, `"icmt"<blob>="..."`.
pub fn comment_date(shown: &str) -> Option<Day> {
    let line = shown.lines().find(|l| l.contains("\"icmt\""))?;
    let (_, tail) = line.split_once('=')?;
    Day::parse(tail.trim().trim_matches('"'))
}

/// A registry's `token_env` read as the store: the secret homma already
/// takes for a publish on a machine with no keychain. It carries no expiry
/// and cannot be written; a write is refused by name.
pub struct Environment {
    /// Registry key to the variable naming its token.
    pub vars: Vec<(String, String)>,
}

impl Environment {
    fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn refusal(&self, key: &str) -> Unavailable {
        Unavailable(format!(
            "this machine has no keychain; {key}'s token is read from {}, which homma does not \
             write: set it where the environment is configured",
            self.var(key)
                .map(|v| format!("`{v}`"))
                .unwrap_or_else(|| format!("`[registries.{key}] token_env`, which is not set")),
        ))
    }
}

impl Store for Environment {
    fn name(&self) -> String {
        "the environment".into()
    }

    fn get(&self, key: &str) -> Result<Option<Stored>, Unavailable> {
        let Some(var) = self.var(key) else {
            return Err(Unavailable(format!(
                "this machine has no keychain, and `[registries.{key}]` names no `token_env` to \
                 read instead"
            )));
        };
        Ok(std::env::var(var)
            .ok()
            .filter(|v| !v.is_empty())
            .map(|secret| {
                Stored {
                    secret,
                    expires: None,
                }
            }))
    }

    fn put(&self, key: &str, _: &str, _: Option<Day>) -> Result<(), Unavailable> {
        Err(self.refusal(key))
    }

    fn writable(&self, key: &str) -> Result<(), Unavailable> {
        Err(self.refusal(key))
    }

    fn forget(&self, key: &str) -> Result<bool, Unavailable> {
        Err(self.refusal(key))
    }
}

/// The store this machine holds tokens in: the keychain on macOS, the
/// environment elsewhere. `vars` is each registry's `token_env`.
pub fn default_store(vars: Vec<(String, String)>) -> Box<dyn Store> {
    #[cfg(target_os = "macos")]
    {
        let _ = vars;
        Box::new(Keychain::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(Environment {
            vars,
        })
    }
}
