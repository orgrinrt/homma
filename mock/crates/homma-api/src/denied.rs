//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The places nothing may be written, wherever the operator points homma.
//!
//! **Containment and this are different questions and thirteen review rounds
//! answered only the first.** [`Root`](crate::Root) asks whether a path stays
//! inside a root the operator named. This asks whether that root was somewhere
//! nothing may be written in the first place, which no amount of containment can
//! answer: a home directory contains itself perfectly.
//!
//! Three kinds of place are denied, and they differ in where each is known from:
//!
//! ```text
//! the operator's own ~/.claude       derived from the home directory
//! another participant's workspace    read from the registry
//! whatever `deny` names              read from the manifest
//! ```
//!
//! The third exists because the first two are the only ones that are the same
//! on every machine. A directory an operator wants homma to stay out of is that
//! operator's arrangement, so it is named in the file that decides everything
//! else about a workspace rather than compiled in here.
//!
//! **The hard part is that `.claude` is not always the denied one.**
//! `.claude/agent-memory/<name>/` inside the content repository is project scope
//! and is written to; `~/.claude` is user scope and is not. Same directory name,
//! opposite verdicts, and when the root is a home they are one directory. So the
//! check is against the operator's own `.claude` by location, never against the
//! bare directory name.
//!
//! The home directory is therefore an **input** rather than something read
//! wherever it is needed. That is what makes this testable without setting a
//! process-global environment variable in a parallel test run.

use std::fmt;

use crate::AbsPath;

/// The absolute locations nothing may be written under.
#[derive(Debug, Clone)]
pub struct Denied {
    entries: Vec<(AbsPath, String)>,
}

/// The home directory, or why it could not be had.
///
/// One reader rather than one per constructor, because every one of them needs
/// it for two different things: deriving the entry that is denied everywhere,
/// and resolving a `~/` in the manifest's own list against the same directory.
/// Two readers would eventually disagree about which home that is.
fn home_from_env() -> Result<AbsPath, NoHome> {
    match std::env::var_os("HOME") {
        None => Err(NoHome::Unset),
        Some(v) => AbsPath::new(&v).map_err(|_| NoHome::Relative(v.to_string_lossy().into_owned())),
    }
}

impl Denied {
    /// The list derived from a home directory.
    ///
    /// Explicit rather than read from the environment, so a test can hand over a
    /// temporary directory and get the real code path rather than a variant of
    /// it.
    pub fn under_home(home: &AbsPath) -> Self {
        Self {
            entries: vec![(
                home.join(".claude"),
                "an assistant's own settings, hooks and credentials live there".to_string(),
            )],
        }
    }

    /// The list for this machine.
    ///
    /// **Fallible, because an unknown home is a licence and not a gap.** This
    /// returned an empty list when `HOME` was absent or relative, under a comment
    /// asserting that was "not a licence". The effect of the home-derived entry
    /// being absent is that the writes it forbids succeed, which is what a
    /// licence is, and both cases were reproduced writing into a `.claude` at
    /// exit 0.
    ///
    /// So a home that cannot be determined stops homma rather than quietly
    /// dropping the one entry that is denied on every machine.
    pub fn from_env() -> Result<Self, NoHome> {
        Ok(Self::under_home(&Self::home()?))
    }

    /// The home directory this machine's list derives from.
    ///
    /// Public because a caller that folds in a manifest's `deny` needs the same
    /// directory to resolve a `~/` against, and reading `HOME` a second time at
    /// the call site is how the two come to disagree.
    pub fn home() -> Result<AbsPath, NoHome> {
        home_from_env()
    }

    /// The whole list for a workspace, with no participant excluded.
    ///
    /// The home-derived entry, every participant's workspace as the registry
    /// gives them, and whatever the manifest's `deny` names. This is what a
    /// caller wants when the thing being written belongs to the workspace rather
    /// than to any participant in it, which the registry file itself does.
    ///
    /// It exists because that caller used to assemble the list by hand from
    /// [`Denied::from_env`] and a loop, and a partial list type-checks wherever
    /// a full one is wanted.
    pub fn for_the_workspace(ws: &crate::Workspace, root: &AbsPath) -> Result<Self, NoHome> {
        let home = Self::home()?;
        let mut denied = Self::under_home(&home).denying(&ws.deny, root, Some(&home));
        for entry in ws.org.values() {
            if let Some(w) = entry.workspace.as_ref() {
                denied = denied.and(AbsPath::resolve(root, w), "it is a participant's workspace");
            }
        }
        Ok(denied)
    }

    /// The two lists a stand-up needs, derived from the registry rather than
    /// from a caller's memory.
    ///
    /// **Another participant's workspace was a loop in `stand.rs`, live and
    /// pinned by nothing**: deleting it left the whole suite passing. The shape
    /// is what allowed that, because [`Denied::from_env`] alone type-checks in
    /// every position where the full list is required, so omitting the registry
    /// was an error nowhere. It is a parameter here, so it cannot be omitted.
    ///
    /// The workspace paths are resolved against `root` exactly as the caller
    /// resolves them, since a registry entry may name a relative one.
    pub fn for_standing_up(
        ws: &crate::Workspace,
        standee: &str,
        root: &AbsPath,
    ) -> Result<Standing, NoHome> {
        let home = Self::home()?;
        let base = Self::under_home(&home).denying(&ws.deny, root, Some(&home));
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
            Some(w) => {
                for_the_workspace.clone().and(
                    w,
                    "it is this participant's own workspace, and a workspace lives \
                 inside the root rather than the other way round",
                )
            },
            None => for_the_workspace.clone(),
        };

        Ok(Standing {
            under_root,
            for_the_workspace,
        })
    }

    /// Add a location denied for a reason other than the home directory.
    ///
    /// Another participant's workspace is the case, and the registry is what
    /// knows where those are rather than the filesystem.
    ///
    /// The reason is owned rather than static, because one of them comes out of
    /// the manifest and a file cannot produce a `&'static str`.
    pub fn and(mut self, path: AbsPath, why: impl Into<String>) -> Self {
        self.entries.push((path, why.into()));
        self
    }

    /// Fold in what a manifest's `deny` names.
    ///
    /// `base` is the directory the manifest sits in, which is what a relative
    /// entry resolves against, the same anchor `local_path` uses. `home` is what
    /// a leading `~/` resolves against; an entry that wants one where the home
    /// is unknown is dropped rather than resolved against something else, since
    /// guessing produces a denial of a place the operator did not name.
    ///
    /// Taken by every constructor below rather than offered as an optional
    /// extra. An earlier entry in this list was a loop at a call site, live and
    /// pinned by nothing, and deleting it left the whole suite passing; the
    /// shape that allowed it was a full list and a partial one both type-checking
    /// in the same position.
    pub fn denying(
        mut self,
        deny: &[crate::DenyEntry],
        base: &AbsPath,
        home: Option<&AbsPath>,
    ) -> Self {
        for entry in deny {
            let Some(path) = entry.resolve(base, home) else {
                continue;
            };
            let why = entry
                .why
                .clone()
                .unwrap_or_else(|| "the workspace manifest denies writes there".to_string());
            self.entries.push((path, why));
        }
        self
    }

    /// Drop every entry denoting the same place as `path`.
    ///
    /// The actor's own workspace is the case. Deny item two is every *other*
    /// participant's workspace, and the home-derived list names one particular
    /// workspace without knowing whose it is, so aggregating into that one was
    /// refused by an entry describing it as somebody else's.
    ///
    /// **The same place, never a place under it.** An entry inside `path` stays
    /// denied: a harness's own `.claude` sitting inside a workspace is still the
    /// harness's, and permitting a workspace must not permit everything in it.
    /// Mutual containment is what says "the same", and it uses both comparisons
    /// [`Denied::check`] uses, so a firmlink spelling and a case-folded spelling
    /// are both recognised. A permission that missed either would leave the
    /// refusal standing under a spelling nobody typed, which is the shape every
    /// reproduction in `check`'s own comment took.
    pub fn permitting(mut self, path: &AbsPath) -> Self {
        let Ok(want) = path.resolved() else {
            // an unresolvable path denotes nothing, so it permits nothing.
            return self;
        };
        self.entries.retain(|(denied, _)| {
            let Ok(there) = denied.resolved() else {
                return true;
            };
            let same = (under_by_components(want.as_path(), there.as_path())
                && under_by_components(there.as_path(), want.as_path()))
                || (under_by_identity(want.as_path(), there.as_path())
                    && under_by_identity(there.as_path(), want.as_path()));
            !same
        });
        self
    }

    /// Refuse a path that resolves under any denied location.
    ///
    /// **Two comparisons, and the split between them is by what the filesystem
    /// can be asked.** [`under_by_components`] answers without consulting it at
    /// all, which is the only thing available for a denied location that does not
    /// exist yet, and it is case-insensitive because an exact one is what folding
    /// defeats. [`under_by_identity`] consults it, which is the only thing that
    /// reaches two spellings sharing no components.
    ///
    /// Three reproductions produced this shape, each writing a workspace, a
    /// repository and a memory link into a denied place at exit 0 with every
    /// containment proof satisfied. `~/.CLAUDE` against `~/.claude`, one inode on
    /// a folding filesystem. `/Users/<user>` against
    /// `/System/Volumes/Data/Users/<user>`, a macOS firmlink, which is not a
    /// symlink and which no amount of link resolution collapses. And a denied
    /// place that did not exist yet, spelled with different case, which the first
    /// version of this pair missed because identity says nothing about a
    /// directory that is not there.
    ///
    /// **No claim is made that these two are exhaustive.** Each was added after
    /// a route through it was demonstrated, and the previous two versions of this
    /// comment each said the pair was total while a route was open.
    pub fn check(&self, path: &AbsPath, what: &str) -> Result<(), Forbidden> {
        let resolved = path.resolved().map_err(|e| {
            Forbidden {
                path:   path.clone(),
                denied: path.clone(),
                what:   what.to_string(),
                why:    format!("it could not be resolved: {e}"),
            }
        })?;
        for (denied, why) in &self.entries {
            let denied_resolved = denied.resolved().map_err(|e| {
                Forbidden {
                    path:   path.clone(),
                    denied: denied.clone(),
                    what:   what.to_string(),
                    why:    format!("it could not be resolved: {e}"),
                }
            })?;
            let under = under_by_components(resolved.as_path(), denied_resolved.as_path())
                || under_by_identity(resolved.as_path(), denied_resolved.as_path());
            if under {
                return Err(Forbidden {
                    path:   path.clone(),
                    denied: denied.clone(),
                    what:   what.to_string(),
                    why:    (*why).to_string(),
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
    pub under_root:        Denied,
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
            NoHome::Unset => {
                write!(
                    f,
                    "HOME is not set, so the places nothing may be written cannot be \
                 determined. Two of the three are derived from it, and running \
                 without them would permit exactly the writes they forbid. Set \
                 HOME to an absolute path."
                )
            },
            NoHome::Relative(v) => {
                write!(
                    f,
                    "HOME is `{v}`, which is relative, so the places nothing may be \
                 written cannot be determined. Set it to an absolute path."
                )
            },
        }
    }
}

impl std::error::Error for NoHome {}

/// Whether `path` is `denied` or lies under it, comparing components without
/// case.
///
/// **This replaced `Path::starts_with`, and the reason is the one the round
/// before this missed.** [`under_by_identity`] answers nothing about a location
/// with no inode, so for a denied place that does not exist yet the component
/// comparison is the only arm that runs, and an exact one is what a folding
/// filesystem defeats. Absent plus folded was answered by neither, and all three
/// deny items were reproduced being written into on exactly that condition. The
/// control is the same command with the directory pre-created, which refuses.
///
/// So the question is not whether two paths *are* one directory, which cannot be
/// asked of a directory that is not there. It is whether they **could** be, and
/// the conservative answer is the right one for a deny list.
///
/// That over-refuses on a case-sensitive filesystem, deliberately: `PROJECTS`
/// beside `projects` would be two directories there and this calls them one.
/// The operator is refused and told why, which is a cheap thing to lose.
// FIXME: Unicode normalisation is not covered. APFS folds NFD against NFC as
// well as case, so `.clauðe` in one form and the other are one directory and two
// spellings here. The standard library has no normalisation and pulling a crate
// in for it is a dependency decision rather than a fix. Every current entry is
// ASCII; a registry-derived workspace path need not be. Unblocked when the
// dependency question is settled.
//
// This list used to say normalisation was the only gap, and two of what it was
// covering for were case rather than normalisation: `straße` against `STRASSE`
// and `Σigma` against `ςigma` are each one directory here, and lowercasing alone
// answers no to both. `folded` closes those, so the sentence above is now true
// as written rather than approximately.
fn under_by_components(path: &std::path::Path, denied: &std::path::Path) -> bool {
    use std::path::Component;
    let mut here = path.components();
    for want in denied.components() {
        let Some(got) = here.next() else {
            return false;
        };
        let same = match (got, want) {
            (Component::Normal(a), Component::Normal(b)) => folded(a) == folded(b),
            (a, b) => a == b,
        };
        if !same {
            return false;
        }
    }
    true
}

/// Case-fold an `OsStr` for comparison, through uppercase and back.
///
/// **Uppercase first, and that is a correctness fix rather than a refinement.**
/// Lowercasing alone answers "not under" for `straße` against `STRASSE`, because
/// `ß` lowercases to itself while uppercasing expands it to `SS`; and for `Σigma`
/// against `ςigma`, because the two sigmas lowercase differently and uppercase
/// to the same letter. Both pairs are one directory on a folding filesystem.
fn folded(s: &std::ffi::OsStr) -> String {
    s.to_string_lossy().to_uppercase().to_lowercase()
}

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
/// False when `denied` does not exist, and **that is a gap rather than a
/// correctness argument**, which is what the previous version of this comment
/// got wrong. It said such a location "is caught by the component comparison
/// beside this". It was not: an exact component comparison is what a folding
/// filesystem defeats, so absent-and-folded fell through both arms and was
/// reproduced writing into all three denied places.
///
/// What covers it now is that the comparison beside this is no longer exact.
/// This one remains for the case components cannot reach at all, where the two
/// spellings share nothing, which is what a firmlink is.
fn under_by_identity(path: &std::path::Path, denied: &std::path::Path) -> bool {
    let Some(target) = identity_of(denied) else {
        return false;
    };
    path.ancestors()
        .any(|a| identity_of(a).is_some_and(|id| id == target))
}

/// A path that lies under a location nothing may be written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forbidden {
    pub path:   AbsPath,
    pub denied: AbsPath,
    what:       String,
    why:        String,
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
    use crate::DenyEntry;
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
            .expect_err("writes under the operator's own .claude are denied");
        assert!(
            err.to_string().contains("credentials"),
            "the message has to say why: {err}"
        );
    }

    /// A list carrying one place a manifest named, resolved the way a manifest
    /// entry is.
    fn denying_one(home: &AbsPath, entry: DenyEntry) -> Denied {
        Denied::under_home(home).denying(std::slice::from_ref(&entry), home, Some(home))
    }

    #[test]
    fn a_place_the_manifest_denies_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        let theirs = home.join("work").join("someone-elses");
        std::fs::create_dir_all(theirs.as_path()).unwrap();

        // The control: nothing but the home-derived entry, and the place is
        // reachable. Without this the assertion below cannot tell a working
        // deny list from one that refuses everything.
        assert!(
            Denied::under_home(&home)
                .check(&theirs.join("x"), "workspace")
                .is_ok(),
            "control: the place is writable before the manifest names it"
        );

        let denied = denying_one(&home, DenyEntry {
            path: std::path::PathBuf::from("work/someone-elses"),
            why:  Some("it belongs to somebody else".to_string()),
        });
        let err = denied
            .check(&theirs.join("x"), "workspace")
            .expect_err("a place the manifest denies is written to");
        assert!(
            err.to_string().contains("it belongs to somebody else"),
            "the manifest's own reason has to reach the operator: {err}"
        );
    }

    #[test]
    fn a_manifest_entry_resolves_a_leading_tilde_against_the_home() {
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        let theirs = home.join("elsewhere");
        std::fs::create_dir_all(theirs.as_path()).unwrap();

        let denied = denying_one(&home, DenyEntry {
            path: std::path::PathBuf::from("~/elsewhere"),
            why:  None,
        });
        assert!(denied.check(&theirs.join("x"), "writing").is_err());
    }

    #[test]
    fn a_manifest_entry_wanting_a_home_is_dropped_when_there_is_none() {
        // Rather than resolved against the manifest's directory, which would
        // deny a place the operator did not name and which nothing in the
        // refusal would explain.
        let d = tempfile::tempdir().unwrap();
        let base = abs(d.path());
        let entry = DenyEntry {
            path: std::path::PathBuf::from("~/elsewhere"),
            why:  None,
        };
        let denied = Denied::under_home(&base).denying(std::slice::from_ref(&entry), &base, None);
        assert!(
            denied
                .check(&base.join("elsewhere").join("x"), "writing")
                .is_ok(),
            "an unresolvable entry denied a place under the manifest instead"
        );
    }

    #[test]
    fn both_spellings_of_a_deny_entry_parse_and_round_trip() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Holder {
            deny: Vec<DenyEntry>,
        }
        let h: Holder =
            toml::from_str("deny = [\"bare/one\", { path = \"other\", why = \"because\" }]\n")
                .expect("both spellings parse");
        assert_eq!(h.deny.len(), 2);
        assert_eq!(h.deny[0].path, std::path::PathBuf::from("bare/one"));
        assert_eq!(h.deny[0].why, None);
        assert_eq!(h.deny[1].why.as_deref(), Some("because"));

        // The one with nothing to carry writes back as the short form, so a
        // manifest homma rewrites reads the way a hand-written one does.
        let back = toml::to_string(&h).unwrap();
        assert!(
            back.contains("\"bare/one\""),
            "the bare entry grew a table: {back}"
        );
        assert!(back.contains("because"), "the reason was dropped: {back}");
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

        assert!(
            Denied::under_home(&home)
                .check(&repo.join(".claude").join("agents"), "definition")
                .is_ok()
        );
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
        assert!(
            denied
                .check(
                    &abs(d.path().join("elsewhere").join("innocent").join("x")),
                    "workspace"
                )
                .is_err()
        );
    }

    #[test]
    fn a_denied_place_that_does_not_exist_still_denies() {
        // Nothing is created by checking, and the home need not carry a
        // `.claude` yet for one to be forbidden.
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        assert!(
            Denied::under_home(&home)
                .check(&home.join(".claude").join("x"), "workspace")
                .is_err()
        );
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

    // **The identity arm, pinned through `check` rather than beside it.**
    //
    // The test above calls `under_by_identity` directly, so deleting the call in
    // `check` left the whole suite green: the function was pinned and its wiring
    // was not, which is this branch's recurring shape and was caught by the
    // deletion sweep rather than by reading.
    //
    // A hard link is the only portable way to give one inode two names that
    // resolution will not collapse, and that is exactly the property a firmlink
    // has: `/Users/<user>` and `/System/Volumes/Data/Users/<user>` are one
    // directory, share no components, and no amount of link resolution turns one
    // into the other. It cannot be created in a test, so this stands in for it.
    #[test]
    fn a_denied_place_reached_under_a_wholly_different_name_is_refused() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a")).unwrap();
        std::fs::create_dir_all(d.path().join("b")).unwrap();
        std::fs::write(d.path().join("a").join("f"), b"x").unwrap();
        std::fs::hard_link(d.path().join("a").join("f"), d.path().join("b").join("g")).unwrap();

        let one = abs(d.path().join("a").join("f"));
        let other = abs(d.path().join("b").join("g"));

        // The precondition, established rather than assumed: components disagree
        // and the filesystem says they are one thing. Without both, this would
        // pass for a reason unrelated to the arm it exists for.
        assert!(!under_by_components(other.as_path(), one.as_path()));
        assert_eq!(identity_of(one.as_path()), identity_of(other.as_path()));

        let denied = Denied::under_home(&abs(d.path().join("nonexistent-home")))
            .and(one, "it is the same thing under another name");
        assert!(
            denied.check(&other, "workspace").is_err(),
            "the component comparison cannot reach this and the identity one must"
        );
    }

    // **The comparison was two disjuncts and is now one, because the sweep found
    // the first unpinnable rather than unpinned.**
    //
    // It read `a.eq_ignore_ascii_case(b) || folded(a) == folded(b)`. Deleting the
    // second left the suite green, which is how it was reported: live and pinned
    // by nothing, in the round about this comparison. Deleting the **first** also
    // leaves it green, and that is a different result: `folded` handles ASCII
    // case too, so `eq_ignore_ascii_case` was a fast path wearing a guard's
    // clothes and no test could ever have failed for its absence.
    //
    // The sweep had missed both because **a compound condition replaced
    // wholesale is one mutation, not two.** Mutating the whole `Normal` arm
    // caught the arm and said nothing about either half.
    #[test]
    fn a_denied_place_spelled_in_ascii_of_another_case_is_refused() {
        // The behaviour, which survives the disjunct that used to carry it.
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        let denied = denying_one(&home, DenyEntry {
            path: std::path::PathBuf::from("work/projects"),
            why:  None,
        });
        assert!(
            denied
                .check(
                    &abs(d.path().join("WORK").join("Projects").join("x")),
                    "root"
                )
                .is_err()
        );
    }

    #[test]
    fn a_denied_place_spelled_with_non_ascii_case_differences_is_refused() {
        // The case an ASCII comparison cannot reach at all: the Kelvin sign
        // against `k`, and `ß` against `SS`, are outside ASCII entirely, and each
        // pair is one directory on a folding filesystem.
        let d = tempfile::tempdir().unwrap();
        let home = abs(d.path());
        let denied = Denied::under_home(&home).and(
            abs(d.path().join("straße")),
            "it stands in for a registry-derived path",
        );

        assert!(
            denied
                .check(&abs(d.path().join("STRASSE").join("x")), "workspace")
                .is_err(),
            "`ß` uppercases to `SS`, and lowercasing alone answers no"
        );
        assert!(
            denied
                .check(&abs(d.path().join("\u{212A}elvin").join("x")), "workspace")
                .is_ok(),
            "and a name that is genuinely different is still allowed"
        );
    }

    #[test]
    fn folding_is_through_uppercase_and_back() {
        // The mechanism, stated where a later reader can see why the order
        // matters. Each of these is one directory on a folding filesystem and
        // `to_lowercase` alone says they are two.
        assert_eq!(folded("stra\u{df}e".as_ref()), folded("STRASSE".as_ref()));
        assert_eq!(
            folded("\u{3a3}igma".as_ref()),
            folded("\u{3c2}igma".as_ref())
        );
        assert_eq!(folded("\u{212a}elvin".as_ref()), folded("kelvin".as_ref()));
        assert_ne!(folded("alpha".as_ref()), folded("beta".as_ref()));
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
        assert!(
            Denied::under_home(&home)
                .check(
                    &abs(d.path().join("Dev").join("crew").join("paja")),
                    "workspace"
                )
                .is_ok()
        );
    }

    // `for_standing_up` reads `HOME`, and these say nothing about it: what they
    // pin is the registry derivation, which is the part that was live and
    // pinned by nothing. The `HOME` cases need their own process and are at
    // `tests/the_home_must_be_known.rs`.
    const TWO_STAFFED: &str = r#"
content_repo = "git@example.invalid:someone/content.git"

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
        assert!(
            s.for_the_workspace
                .check(&abs("/srv/vouti/inside"), "workspace")
                .is_err()
        );
        assert!(
            s.under_root
                .check(&abs("/srv/vouti/inside"), "workspace root")
                .is_err()
        );
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
content_repo = "git@example.invalid:someone/content.git"

[org.op]
role = "king"
handle = "op"
"#,
        )
        .unwrap();
        let s = Denied::for_standing_up(&ws, "paja", &abs("/srv")).unwrap();
        assert!(s.under_root.check(&abs("/srv/anywhere"), "root").is_ok());
    }

    /// `home` plus one place a manifest denied, which is the two-entry list
    /// every test below needs to tell "permitted one" from "emptied the list".
    fn two_entry_list(home: &AbsPath, rel: &str) -> Denied {
        denying_one(home, DenyEntry {
            path: std::path::PathBuf::from(rel),
            why:  Some("it belongs to somebody else".to_string()),
        })
    }

    #[test]
    fn permitting_drops_the_entry_for_that_place_and_nothing_else() {
        let d = tempfile::tempdir().unwrap();
        let home = AbsPath::new(d.path().canonicalize().unwrap()).unwrap();
        let ws = home.join("work").join("theirs");
        std::fs::create_dir_all(ws.as_path()).unwrap();
        std::fs::create_dir_all(home.join(".claude").as_path()).unwrap();

        // the control first: without the permission the workspace is refused,
        // by an entry that describes it as somebody else's.
        let before = two_entry_list(&home, "work/theirs");
        assert!(
            before.check(&ws.join(".claude"), "aggregating").is_err(),
            "control: the workspace is denied to begin with"
        );

        let after = two_entry_list(&home, "work/theirs").permitting(&ws);
        assert!(
            after.check(&ws.join(".claude"), "aggregating").is_ok(),
            "the workspace is still refused after being permitted"
        );
        // and the other home-derived entry is untouched, so this permitted one
        // place rather than emptying the list.
        assert!(
            after.check(&home.join(".claude"), "aggregating").is_err(),
            "permitting one place emptied the list"
        );
    }

    #[test]
    fn permitting_a_place_does_not_permit_a_denied_place_inside_it() {
        // the asymmetry the doc claims. A harness `.claude` that happens to sit
        // inside the workspace is still the harness's, so permitting the
        // workspace must not reach it.
        let d = tempfile::tempdir().unwrap();
        let home = AbsPath::new(d.path().canonicalize().unwrap()).unwrap();
        std::fs::create_dir_all(home.join(".claude").as_path()).unwrap();

        // the home itself as the "workspace": the `.claude` entry is under it.
        let after = Denied::under_home(&home).permitting(&home);
        assert!(
            after
                .check(&home.join(".claude").join("x"), "writing")
                .is_err(),
            "permitting a place permitted a denied place inside it"
        );
    }

    #[test]
    fn permitting_a_place_inside_an_entry_leaves_the_entry_standing() {
        // the other direction, and the one that would quietly open the whole
        // deny list if `permitting` tested containment instead of sameness.
        let d = tempfile::tempdir().unwrap();
        let home = AbsPath::new(d.path().canonicalize().unwrap()).unwrap();
        let inside = home.join("work").join("theirs").join("member");
        std::fs::create_dir_all(inside.as_path()).unwrap();

        let after = two_entry_list(&home, "work/theirs").permitting(&inside);
        assert!(
            after.check(&inside.join("x"), "writing").is_err(),
            "a path inside a denied place permitted itself out of it"
        );
    }

    #[test]
    fn permitting_matches_a_case_folded_spelling_of_the_same_place() {
        // the spelling nobody types, which is the shape every reproduction in
        // `check`'s own comment took. Skipped where the filesystem does not
        // fold, since there the two spellings are genuinely two places.
        let d = tempfile::tempdir().unwrap();
        let home = AbsPath::new(d.path().canonicalize().unwrap()).unwrap();
        let ws = home.join("work").join("theirs");
        std::fs::create_dir_all(ws.as_path()).unwrap();
        let folded = home.join("work").join("THEIRS");
        if !folded.as_path().is_dir() {
            return; // the filesystem does not fold; nothing to test here
        }

        let after = two_entry_list(&home, "work/theirs").permitting(&folded);
        assert!(
            after.check(&ws.join(".claude"), "aggregating").is_ok(),
            "a case-folded spelling of the same place did not permit it"
        );
    }

    #[test]
    fn permitting_something_the_list_does_not_name_changes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let home = AbsPath::new(d.path().canonicalize().unwrap()).unwrap();
        std::fs::create_dir_all(home.join(".claude").as_path()).unwrap();
        let elsewhere = home.join("unrelated");
        std::fs::create_dir_all(elsewhere.as_path()).unwrap();

        let after = two_entry_list(&home, "work/theirs").permitting(&elsewhere);
        assert!(after.check(&home.join(".claude"), "writing").is_err());
        assert!(
            after
                .check(&home.join("work").join("theirs"), "writing")
                .is_err()
        );
    }
}
