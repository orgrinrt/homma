//! Shared test support.
//!
//! One home for the helpers, because the last round fixed a vacuous assertion in
//! one copy and left the copy beside it untouched. A report names an instance;
//! the fix is the class.

use std::path::PathBuf;

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

/// A snapshot of every global configuration that **exists**, for asserting none
/// of them changed.
///
/// Panics when none exists, rather than returning an empty snapshot that
/// compares equal to another empty snapshot. That is what the earlier version
/// did: `/etc/gitconfig` was pushed unconditionally so the list was never empty,
/// the file does not exist here, and the comparison ran over `[None]` and
/// passed having checked nothing.
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
