//! The places nothing may be written, wherever the operator points homma.
//!
//! **Containment and this are different questions and thirteen review rounds
//! answered only the first.** [`Root`](crate::Root) asks whether a path stays
//! inside a root the operator named. This asks whether that root was somewhere
//! the record forbids in the first place, which no amount of containment can
//! answer: a home directory contains itself perfectly.
//!
//! The record's list is three absolute locations:
//!
//! ```text
//! 1  writes under ~/Dev/clause-dev      the central clone
//! 2  writes under another Hand's workspace
//! 3  writes under ~/.claude/            settings, hooks, live credentials
//! ```
//!
//! Two derive from the home directory and one from the registry.
//!
//! **The hard part is that `.claude` is not always the denied one.** The record
//! licenses `.claude/agent-memory/<name>/` inside the content repository, which
//! is project scope, and denies `~/.claude`, which is user scope. Same directory
//! name, opposite verdicts, and when the root is a home they are one directory.
//! So the check is against the operator's own `.claude` by absolute path, never
//! against the name.
//!
//! The home directory is therefore an **input** rather than something read
//! wherever it is needed. That is what makes this testable without setting a
//! process-global environment variable in a parallel test run.

use crate::AbsPath;
use std::fmt;

/// The absolute locations nothing may be written under.
#[derive(Debug, Clone)]
pub struct Denied {
    entries: Vec<(AbsPath, &'static str)>,
}

impl Denied {
    /// The list derived from a home directory.
    ///
    /// Explicit rather than read from the environment, so a test can hand over a
    /// temporary directory and get the real code path rather than a variant of
    /// it.
    pub fn under_home(home: &AbsPath) -> Self {
        Self {
            entries: vec![
                (
                    home.join(".claude"),
                    "the agent harness's own settings, hooks and credentials live there",
                ),
                (
                    home.join("Dev").join("clause-dev"),
                    "the central clone is read, never written",
                ),
            ],
        }
    }

    /// The list for this machine, or an empty one when there is no home.
    ///
    /// An absent `HOME` is not an error and is not a licence: it means the two
    /// home-derived entries cannot be computed, so they are absent and the
    /// registry-derived ones still apply. Refusing outright would make homma
    /// unusable in an environment that has no home and writes nowhere near one.
    pub fn from_env() -> Self {
        match std::env::var_os("HOME").map(AbsPath::new) {
            Some(Ok(home)) => Self::under_home(&home),
            _ => Self { entries: vec![] },
        }
    }

    /// Add a location denied for a reason other than the home directory.
    ///
    /// Deny item two is every other participant's workspace, which is known from
    /// the registry rather than from the filesystem.
    pub fn and(mut self, path: AbsPath, why: &'static str) -> Self {
        self.entries.push((path, why));
        self
    }

    /// Refuse a path that resolves under any denied location.
    ///
    /// Resolved on both sides, because a symlink is how a path reaches a place
    /// it does not appear to name, and that is the whole history of this branch.
    pub fn check(&self, path: &AbsPath, what: &str) -> Result<(), Forbidden> {
        let resolved = path.resolved().map_err(|e| Forbidden {
            path: path.clone(),
            denied: path.clone(),
            what: what.to_string(),
            why: format!("it could not be resolved: {e}"),
        })?;
        for (denied, why) in &self.entries {
            let denied_resolved = denied.resolved().map_err(|e| Forbidden {
                path: path.clone(),
                denied: denied.clone(),
                what: what.to_string(),
                why: format!("it could not be resolved: {e}"),
            })?;
            if resolved.as_path().starts_with(denied_resolved.as_path()) {
                return Err(Forbidden {
                    path: path.clone(),
                    denied: denied.clone(),
                    what: what.to_string(),
                    why: (*why).to_string(),
                });
            }
        }
        Ok(())
    }

    /// Whether anything is denied at all, for a caller that wants to say so.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A path that lies under a location the record denies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forbidden {
    pub path: AbsPath,
    pub denied: AbsPath,
    what: String,
    why: String,
}

impl fmt::Display for Forbidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is under {}, and nothing may be written there: {}. \
             Name a {} somewhere else.",
            self.path, self.denied, self.why, self.what
        )
    }
}

impl std::error::Error for Forbidden {}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(p: impl Into<std::path::PathBuf>) -> AbsPath {
        AbsPath::new(p).expect("a tempdir path is absolute")
    }

    #[test]
    fn a_path_under_the_operators_claude_directory_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        std::fs::create_dir_all(d.path().join(".claude")).unwrap();
        let denied = Denied::under_home(&home);

        let err = denied
            .check(&home.join(".claude").join("crewroot"), "workspace root")
            .expect_err("the record denies writes under the operator's own .claude");
        assert!(
            err.to_string().contains("credentials"),
            "the message has to say why: {err}"
        );
    }

    #[test]
    fn the_central_clone_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        std::fs::create_dir_all(d.path().join("Dev").join("clause-dev")).unwrap();
        assert!(Denied::under_home(&home)
            .check(&home.join("Dev").join("clause-dev").join("x"), "workspace")
            .is_err());
    }

    // The distinction the whole type exists for. `.claude` inside a content
    // repository is project scope and licensed; `~/.claude` is user scope and
    // denied. Checking the name rather than the location would refuse both.
    #[test]
    fn a_claude_directory_that_is_not_the_operators_is_allowed() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path().join("home"));
        std::fs::create_dir_all(d.path().join("home").join(".claude")).unwrap();
        let repo = abs(d.path().join("repo"));
        std::fs::create_dir_all(d.path().join("repo").join(".claude")).unwrap();

        assert!(Denied::under_home(&home)
            .check(&repo.join(".claude").join("agents"), "definition")
            .is_ok());
    }

    // A symlink is how a path reaches somewhere it does not appear to name, and
    // that is the entire history of this branch.
    #[test]
    fn a_symlink_into_the_denied_place_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path().join("home"));
        std::fs::create_dir_all(d.path().join("home").join(".claude")).unwrap();
        std::fs::create_dir_all(d.path().join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(
            d.path().join("home").join(".claude"),
            d.path().join("elsewhere").join("innocent"),
        )
        .unwrap();

        let denied = Denied::under_home(&home);
        assert!(denied
            .check(
                &abs(d.path().join("elsewhere").join("innocent").join("x")),
                "workspace"
            )
            .is_err());
    }

    #[test]
    fn a_denied_place_that_does_not_exist_still_denies() {
        // Nothing is created by checking, and the home need not carry a
        // `.claude` yet for one to be forbidden.
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        assert!(Denied::under_home(&home)
            .check(&home.join(".claude").join("x"), "workspace")
            .is_err());
        assert!(!d.path().join(".claude").exists());
    }

    #[test]
    fn an_added_location_is_denied_too() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path().join("home"));
        let other = abs(d.path().join("someone-elses-workspace"));
        let denied = Denied::under_home(&home).and(other.clone(), "it belongs to another Hand");
        assert!(denied.check(&other.join("inside"), "workspace").is_err());
    }

    #[test]
    fn an_ordinary_path_is_allowed() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path().join("home"));
        assert!(Denied::under_home(&home)
            .check(
                &abs(d.path().join("Dev").join("crew").join("paja")),
                "workspace"
            )
            .is_ok());
    }
}
