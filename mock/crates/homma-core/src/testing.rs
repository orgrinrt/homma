//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Helpers shared by tests, in the library so there is exactly one of them.
//!
//! `global_config_paths` existed twice, was fixed in one copy, and the round
//! that fixed it added a third. Two of the three were in different crates, which
//! is why copying looked like the only option; a `pub` module is the other one.
//!
//! Not `#[cfg(test)]`: a unit test in this crate and an integration test in
//! another both need it, and `cfg(test)` does not cross a crate boundary.

use std::path::{Path, PathBuf};

/// Every place git would read a configuration that is not a repository's own.
pub fn global_config_paths() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/etc/gitconfig")];
    if let Ok(v) = std::env::var("GIT_CONFIG_GLOBAL") {
        out.push(PathBuf::from(v));
    }
    if let Ok(v) = std::env::var("XDG_CONFIG_HOME") {
        out.push(PathBuf::from(v).join("git").join("config"));
    }
    if let Ok(v) = std::env::var("HOME") {
        out.push(PathBuf::from(&v).join(".gitconfig"));
        out.push(PathBuf::from(&v).join(".config").join("git").join("config"));
    }
    out
}

/// A snapshot of every global configuration that **exists**, for asserting that
/// none of them changed.
///
/// Panics when none exists rather than returning an empty snapshot, because an
/// empty snapshot compares equal to another empty one and the assertion then
/// checks nothing. That is what the first version did: `/etc/gitconfig` is
/// pushed unconditionally so the list was never empty, the file does not exist
/// on every machine, and the comparison ran over `[None]` and passed.
pub fn global_configs_now() -> Vec<(PathBuf, Vec<u8>)> {
    let found: Vec<_> = global_config_paths()
        .into_iter()
        .filter_map(|p| std::fs::read(&p).ok().map(|c| (p, c)))
        .collect();
    assert!(
        !found.is_empty(),
        "no global git configuration exists, so this test would assert nothing"
    );
    found
}

/// Make `dir` a real repository, so anything reading it reads what git would.
///
/// Here rather than in each test crate because the hook-wiring probe is read
/// through gix, and only a genuine repository exercises that path. A
/// hand-written `.git` would be a fixture shaped to satisfy the reader rather
/// than a repository, which is the setup-that-helps failure.
pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).expect("a temp dir is creatable");
    gix::init(dir).expect("an empty directory is a legitimate repository");
}

/// Point a repository's `core.hooksPath` at `value`.
///
/// Appended as text because that is what git itself writes, which keeps this
/// independent of whichever gix version owns the write path.
pub fn set_hooks_path(dir: &Path, value: &str) {
    let at = dir.join(".git").join("config");
    let mut body = std::fs::read_to_string(&at).unwrap_or_default();
    body.push_str(&format!("[core]\n\thooksPath = {value}\n"));
    std::fs::write(at, body).expect("the repository's own config is writable");
}

/// The four variables git reads an identity from before it reads any config key.
///
/// One list, because the same four names were written out in three test files.
pub const GIT_IDENTITY_ENV: [&str; 4] = [
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
];

/// Take the machine's identity variables out of a `git` a test spawns.
///
/// For a test about something other than the identity: one that asks what a
/// configuration holds, or builds a repository to look at. A git author or
/// committer variable outranks every config key, so a machine exporting them
/// would answer in place of the configuration under test. A test about the
/// identity itself does not use this, it uses [`hostile_git_identity_env`].
pub fn without_git_identity_env(cmd: &mut std::process::Command) -> &mut std::process::Command {
    for k in GIT_IDENTITY_ENV {
        cmd.env_remove(k);
    }
    cmd
}

/// Put somebody else's identity in a `git`'s environment, at both tiers above the
/// configuration files: the four identity variables, and a `GIT_CONFIG_COUNT`
/// entry naming an `author.name`.
///
/// What `hostile_config` does for the files, so a test of an identity fails on
/// every machine and not only on the ones that export these.
pub fn hostile_git_identity_env(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd.env("GIT_AUTHOR_NAME", "Env Author")
        .env("GIT_AUTHOR_EMAIL", "env-author@example.invalid")
        .env("GIT_COMMITTER_NAME", "Env Committer")
        .env("GIT_COMMITTER_EMAIL", "env-committer@example.invalid")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "author.name")
        .env("GIT_CONFIG_VALUE_0", "Env Config Author")
}
