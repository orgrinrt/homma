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

    /// The list for this machine.
    ///
    /// **Fallible, because an unknown home is a licence and not a gap.** This
    /// returned an empty list when `HOME` was absent or relative, under a comment
    /// asserting that was "not a licence". The effect of the two home-derived
    /// entries being absent is that the writes they forbid succeed, which is
    /// what a licence is, and both cases were reproduced writing into a
    /// `.claude` at exit 0.
    ///
    /// So a home that cannot be determined stops homma rather than quietly
    /// removing two thirds of the list.
    pub fn from_env() -> Result<Self, NoHome> {
        match std::env::var_os("HOME") {
            None => Err(NoHome::Unset),
            Some(v) => match AbsPath::new(&v) {
                Ok(home) => Ok(Self::under_home(&home)),
                Err(_) => Err(NoHome::Relative(v.to_string_lossy().into_owned())),
            },
        }
    }

    /// The two lists a stand-up needs, derived from the registry rather than
    /// from a caller's memory.
    ///
    /// **Deny item two was a loop in `stand.rs`, live and pinned by nothing**:
    /// deleting it left the whole suite passing. The shape is what allowed that,
    /// because [`Denied::from_env`] alone type-checks in every position where the
    /// full list is required, so omitting the registry was an error nowhere. It
    /// is a parameter here, so it cannot be omitted.
    ///
    /// The workspace paths are resolved against `root` exactly as the caller
    /// resolves them, since a registry entry may name a relative one.
    pub fn for_standing_up(
        ws: &crate::Workspace,
        standee: &str,
        root: &AbsPath,
    ) -> Result<Standing, NoHome> {
        let base = Self::from_env()?;
        let mut for_the_workspace = base.clone();
        let mut own: Option<AbsPath> = None;

        for (handle, entry) in ws.org.iter() {
            let Some(w) = entry.workspace.as_ref() else {
                continue;
            };
            let resolved = AbsPath::resolve(root, w);
            if handle == standee {
                own = Some(resolved);
            } else {
                for_the_workspace =
                    for_the_workspace.and(resolved, "it is another participant's workspace");
            }
        }

        // The standee's own workspace is denied under the root and not on the
        // list the workspace itself is checked against, which would deny itself.
        //
        // A root inside a workspace is backwards: the workspace is required to
        // sit outside the root, so nothing derived under the root can reach it
        // except through a link, which is the case worth refusing. Skipping the
        // standee let a root inside its own workspace through, `git.init` ran,
        // and the run failed in `provision` having left a `.git` behind.
        let under_root = match own {
            Some(w) => for_the_workspace.clone().and(
                w,
                "it is this participant's own workspace, and a workspace lives \
                 inside the root rather than the other way round",
            ),
            None => for_the_workspace.clone(),
        };

        Ok(Standing {
            under_root,
            for_the_workspace,
        })
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
    /// **Two comparisons, and neither subsumes the other.** Components, which
    /// answers for a denied location that does not exist yet and therefore has no
    /// identity to compare. And `(dev, ino)`, which answers for a location that
    /// has two names.
    ///
    /// The second is what the fifteenth review reproduced twice. This filesystem
    /// is case-insensitive, so `~/.CLAUDE` and `~/.claude` are one inode and a
    /// component comparison says no; and `/Users/<user>` is a macOS firmlink to
    /// `/System/Volumes/Data/Users/<user>`, which is not a symlink, so no amount
    /// of link resolution collapses it. Both wrote a workspace, a repository and
    /// a memory link inside the operator's own `.claude` at exit 0, with every
    /// containment proof satisfied.
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
            let under = resolved.as_path().starts_with(denied_resolved.as_path())
                || under_by_identity(resolved.as_path(), denied_resolved.as_path());
            if under {
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
}

/// The two lists standing one participant up requires.
///
/// A struct rather than one list, because the standee's own workspace belongs on
/// one of them and not the other, and a single list would have to be silently
/// wrong for one of the two questions.
#[derive(Debug, Clone)]
pub struct Standing {
    /// Everything forbidden under the root, every workspace included.
    pub under_root: Denied,
    /// The same without the standee's own workspace, for checking that
    /// workspace, which would otherwise deny itself.
    pub for_the_workspace: Denied,
}

/// Why the home-derived deny entries could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoHome {
    Unset,
    Relative(String),
}

impl fmt::Display for NoHome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoHome::Unset => write!(
                f,
                "HOME is not set, so the places nothing may be written cannot be \
                 determined. Two of the three are derived from it, and running \
                 without them would permit exactly the writes they forbid. Set \
                 HOME to an absolute path."
            ),
            NoHome::Relative(v) => write!(
                f,
                "HOME is `{v}`, which is relative, so the places nothing may be \
                 written cannot be determined. Set it to an absolute path."
            ),
        }
    }
}

impl std::error::Error for NoHome {}

/// What a directory actually is, when it exists.
///
/// A path is a way of writing a directory down and a filesystem accepts several
/// spellings for one: case folding and firmlinks are two, and a bind mount would
/// be a third. `(dev, ino)` is the directory.
fn identity_of(p: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).ok().map(|m| (m.dev(), m.ino()))
}

/// Whether any ancestor of `path` is the same directory as `denied`.
///
/// Ancestors rather than `path` itself, because the leaf is usually about to be
/// created and does not exist, so the answer lives at the deepest existing
/// prefix. Ones that do not exist are skipped rather than ending the walk: a
/// path may name a missing directory below an existing one, which is the
/// ordinary case here.
///
/// False when `denied` does not exist, which is correct rather than lenient. A
/// location with no identity is caught by the component comparison beside this,
/// and one of the tests asserts a denied place that does not exist still denies.
fn under_by_identity(path: &std::path::Path, denied: &std::path::Path) -> bool {
    let Some(target) = identity_of(denied) else {
        return false;
    };
    path.ancestors()
        .any(|a| identity_of(a).is_some_and(|id| id == target))
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

    // **The defect that made every containment proof beside the point.** One
    // character in a `--root` argument put a workspace, a repository, both
    // definitions and the memory link inside the operator's own `.claude`, at
    // exit 0, because `starts_with` compares components and this filesystem
    // folds case.
    //
    // It establishes that the two spellings reach one directory before asserting
    // anything, so on a case-sensitive filesystem it reports that and stops
    // rather than passing for the wrong reason. A test that cannot tell a real
    // pass from an inapplicable one is the shape this branch has shipped
    // repeatedly.
    #[test]
    fn a_differently_cased_spelling_of_a_denied_place_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        std::fs::create_dir_all(d.path().join(".claude")).unwrap();

        let folded = d.path().join(".CLAUDE");
        let Some(one) = identity_of(&folded) else {
            eprintln!(
                "skipped: {} is case-sensitive, so the two spellings are two \
                 directories and there is nothing here to catch",
                d.path().display()
            );
            return;
        };
        assert_eq!(
            Some(one),
            identity_of(&d.path().join(".claude")),
            "the two spellings must reach one inode for this to be testing anything"
        );

        let err = Denied::under_home(&home)
            .check(&abs(folded.join("crewroot")), "workspace root")
            .expect_err("a denied directory reached by another spelling is the same directory");
        assert!(err.to_string().contains("credentials"), "{err}");
    }

    // The mechanism the case above exercises, stated portably and without
    // depending on how this filesystem spells things.
    //
    // A hard link is the one way to give two paths one inode that resolution
    // cannot collapse, which is precisely the property a firmlink has and a
    // symlink does not: `/Users/<user>` is a firmlink to
    // `/System/Volumes/Data/Users/<user>`, `realpath` leaves it alone, and the
    // two spellings differ in every component.
    #[test]
    fn two_paths_that_are_one_thing_are_recognised_as_one_thing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a")).unwrap();
        std::fs::create_dir_all(d.path().join("b")).unwrap();
        std::fs::write(d.path().join("a").join("f"), b"x").unwrap();
        std::fs::hard_link(d.path().join("a").join("f"), d.path().join("b").join("g")).unwrap();

        let one = d.path().join("a").join("f");
        let other = d.path().join("b").join("g");

        assert!(
            !other.starts_with(&one),
            "the two spellings must disagree by component, or this proves nothing"
        );
        assert_eq!(
            identity_of(&one),
            identity_of(&other),
            "a hard link is one inode with two names"
        );
        assert!(under_by_identity(&other, &one));
    }

    #[test]
    fn identity_says_nothing_about_a_place_that_does_not_exist() {
        // Which is why the component comparison stays beside it rather than
        // being replaced by it, and why `a_denied_place_that_does_not_exist_
        // still_denies` above is not redundant with this one.
        let d = tempfile::tempdir().unwrap();
        assert!(!under_by_identity(
            &d.path().join("absent").join("x"),
            &d.path().join("absent")
        ));
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

    // `for_standing_up` reads `HOME`, and these say nothing about it: what they
    // pin is the registry derivation, which is the part that was live and
    // pinned by nothing. The `HOME` cases need their own process and are at
    // `tests/the_home_must_be_known.rs`.
    const TWO_STAFFED: &str = r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"

[org.paja]
role = "hand"
staffed = true
handle = "paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/srv/paja"

[org.vouti]
role = "hand"
staffed = true
handle = "vouti"
git_name = "vouti"
git_email = "vouti@example.invalid"
workspace = "/srv/vouti"
"#;

    fn standing(standee: &str) -> Standing {
        let ws = crate::Workspace::parse(TWO_STAFFED).expect("the fixture parses");
        Denied::for_standing_up(&ws, standee, &abs("/srv")).expect("HOME is set in a test process")
    }

    #[test]
    fn another_participants_workspace_is_denied() {
        // The whole of deny item two, which had no test anywhere in the
        // production path: no fixture in the tree built a registry with two
        // staffed participants, so deleting the derivation left everything
        // green.
        let s = standing("paja");
        assert!(s
            .for_the_workspace
            .check(&abs("/srv/vouti/inside"), "workspace")
            .is_err());
        assert!(s
            .under_root
            .check(&abs("/srv/vouti/inside"), "workspace root")
            .is_err());
    }

    #[test]
    fn a_participants_own_workspace_is_denied_under_the_root_and_not_to_itself() {
        // The two questions, and why one list cannot answer both. A root inside
        // any workspace is backwards, this participant's own included; the
        // workspace checked against a list carrying itself would deny itself.
        let s = standing("paja");
        assert!(
            s.under_root
                .check(&abs("/srv/paja/root"), "workspace root")
                .is_err(),
            "a root inside the standee's own workspace is still a root inside a workspace"
        );
        assert!(
            s.for_the_workspace
                .check(&abs("/srv/paja"), "workspace")
                .is_ok(),
            "the workspace must not deny itself"
        );
    }

    #[test]
    fn an_entry_with_no_workspace_contributes_nothing() {
        let ws = crate::Workspace::parse(
            r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"

[org.op]
role = "king"
handle = "op"
"#,
        )
        .unwrap();
        let s = Denied::for_standing_up(&ws, "paja", &abs("/srv")).unwrap();
        assert!(s.under_root.check(&abs("/srv/anywhere"), "root").is_ok());
    }
}
