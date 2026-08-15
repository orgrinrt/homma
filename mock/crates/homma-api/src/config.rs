//! The registry: who exists, and what each of them needs to act.
//!
//! Identity is a configuration entry, never a running process. A participant
//! outlives whatever was running it, so starting one again restores a
//! participant rather than creating a new one.

use crate::path::RelPath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What kind of participant an entry describes.
///
/// The roles are homma's, not any workspace's. What a workspace calls its
/// participants and what domains it gives them is configuration; that some are
/// people, some are standing agents, some are consulted and some are labour is
/// the structure homma itself reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The human. The only source of ratification.
    King,
    /// A standing agent with a workspace, a memory and an identity of its own.
    Hand,
    /// Dispatched for a question, never resident. Has memory, no workspace.
    Expert,
    /// Utility labour. No persona, no memory, no workspace.
    General,
}

impl Role {
    /// Whether a participant in this role owns a workspace on disk.
    pub fn has_workspace(self) -> bool {
        matches!(self, Role::Hand)
    }

    /// Whether a participant in this role accumulates version-controlled memory.
    pub fn has_memory(self) -> bool {
        matches!(self, Role::Hand | Role::Expert)
    }
}

/// Whether an identity that could own a workspace has somebody on it.
///
/// Named staffing rather than **standing**, because *standing* already means
/// what a reference derives, one module over, and a vocabulary crate using one
/// word for two concepts has failed at its only job.
///
/// The blind global replace that performed that rename corrupted this very
/// comment into "named staffing rather than staffing because staffing already
/// means", three words after the sentence above.
///
/// The distinction between [`Staffing::Mapped`] and [`Staffing::Incomplete`] is
/// the whole reason this is an enum rather than a list of absent fields: a mapped
/// entry lacks exactly the fields an unfinished one lacks, so any answer of the
/// shape "here is what is missing" conflates a decision with a mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staffing {
    /// The role owns no workspace at all, so there is nothing to stand up and no
    /// set of fields that would change that.
    NoWorkspace,
    /// A boundary recorded, so the same ground is not carved twice. Complete as
    /// an entry, and deliberately without a workspace.
    Mapped,
    /// Meant to have a workspace, and these fields are absent.
    Incomplete(Vec<&'static str>),
    /// Ready to be stood up.
    Staffed,
}

impl Staffing {
    /// Whether a workspace may be created for this identity.
    pub fn can_stand_up(&self) -> bool {
        matches!(self, Staffing::Staffed)
    }
}

/// One participant.
///
/// A consultant's entry carries a role and a definition and nothing else,
/// because it has no workspace, no session, no git identity and no repositories.
/// The optional fields are optional for that reason rather than for tidiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub role: Role,
    /// What everything else addresses it by.
    pub handle: String,
    /// Whether this entry is meant to have a workspace.
    ///
    /// Declared rather than inferred from an absent workspace. Inference is
    /// cheaper and wrong: an entry whose workspace was mistyped or dropped would
    /// read as a deliberate decision, which is the confusion this removes.
    #[serde(default)]
    pub staffed: bool,
    /// The short name a human uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// The full form, for when the short one is too familiar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// What it is good at. A default for routing and an answer to who to ask,
    /// never a partition: work is assigned, not confined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_email: Option<String>,
    /// The address commits are *committed* by, when it differs from the author.
    ///
    /// **Absent means the committer is the author**, which is every ordinary
    /// entry. Present means one clone carries two identities, which the record
    /// settles for Vouti alone: the author stays op so attribution is his, and
    /// the committer is a tagged address on his own so it "just works" while
    /// distinguishing what the crew wrote.
    ///
    /// Optional rather than defaulted to the author in the type, because a
    /// default here would make "the same" and "deliberately the same"
    /// indistinguishable in the file, and the file is what a human reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_email: Option<String>,
    /// The name commits are *committed* by, when it differs from the author's.
    ///
    /// Shipped a round late. U-3.2 names an optional committer name and email;
    /// only the email arrived, and git treats `committer.name` as a first-class
    /// key that a global one can override, so the omission was a hole rather
    /// than a simplification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_name: Option<String>,
    /// Where its work happens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Pinned, so restarting reaches the same participant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Added as work needs them rather than up front.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
}

impl Identity {
    pub fn new(role: Role, handle: impl Into<String>) -> Self {
        Self {
            role,
            handle: handle.into(),
            staffed: false,
            nickname: None,
            full_name: None,
            domain: None,
            git_name: None,
            git_email: None,
            committer_email: None,
            committer_name: None,
            workspace: None,
            session: None,
            repos: Vec::new(),
        }
    }

    /// Whether this identity has somebody on it, and therefore whether it can
    /// be stood up.
    ///
    /// Replaces an earlier `missing()` that returned a list of absent fields.
    /// That shape cannot express the distinction that matters, because an entry
    /// which is mapped on purpose lacks exactly the fields an unfinished one
    /// lacks, so an empty-or-not answer reports a decision as a mistake.
    pub fn staffing(&self) -> Staffing {
        if !self.role.has_workspace() {
            // Not a gap that filling fields would close. Reported so a caller
            // refuses rather than building one anyway, which under the king's
            // handle would manufacture a dispatchable agent for the human.
            return Staffing::NoWorkspace;
        }
        if !self.staffed {
            return Staffing::Mapped;
        }
        let mut gaps = Vec::new();
        if self.git_name.is_none() {
            gaps.push("git_name");
        }
        if self.git_email.is_none() {
            gaps.push("git_email");
        }
        if self.workspace.is_none() {
            gaps.push("workspace");
        }
        if gaps.is_empty() {
            Staffing::Staffed
        } else {
            Staffing::Incomplete(gaps)
        }
    }
}

/// Where homma keeps things in a workspace.
///
/// Every path has a default homma will create and an override for a workspace
/// whose convention differs. Nothing about the workspace homma was born in is
/// compiled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub hands: RelPath,
    pub experts: RelPath,
    pub channels: RelPath,
    pub agents: RelPath,
    pub index: RelPath,
}

impl Default for Paths {
    fn default() -> Self {
        // Every one of these is a literal in this file, so `expect` here says
        // "this source is wrong" rather than "the configuration is wrong",
        // which is the only way it can fail.
        let rel = |s: &str| RelPath::new(s).expect("a default path is relative and contained");
        Self {
            hands: rel(".shared/hands"),
            experts: rel(".shared/experts"),
            channels: rel(".shared/channels"),
            agents: rel(".claude/agents"),
            index: rel(".shared/.index"),
        }
    }
}

/// The literal meaning "this directory is the content repository".
pub const LOCAL: &str = "local";

fn local() -> String {
    LOCAL.to_string()
}

/// What homma needs of a workspace, all of which has a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Where the repository holding workspace metadata and content lives.
    ///
    /// A git URI, or the literal `local`, which is the default.
    ///
    /// A URI rather than a name, because it is what a workspace is cloned from.
    /// A name would have to be resolved into one, and the round that tried
    /// resolved it from whatever repository the operator happened to be standing
    /// in, which cloned that repository instead.
    ///
    /// `local` means the configuration's own directory **is** the content
    /// repository, initialised if it is not one yet. That is the shape a
    /// workspace starts in before it has a remote, so it is the default and
    /// needs no key at all.
    #[serde(default = "local")]
    pub content_repo: String,
    #[serde(default)]
    pub paths: Paths,
    /// Handle to entry.
    #[serde(default)]
    pub org: BTreeMap<String, Identity>,
}

impl Workspace {
    /// The role a handle holds, for deriving a record's standing.
    pub fn role_of(&self, handle: &str) -> Option<Role> {
        self.org.get(handle).map(|i| i.role)
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Registry strings that would inject structure into anything generated
    /// from them.
    ///
    /// A generated agent definition puts these into YAML frontmatter, so a
    /// newline in a nickname writes an arbitrary key. That is how a Hand could
    /// grant its own twin the memory key the design withholds structurally, by
    /// editing its own entry in a file every participant can write.
    pub fn unsafe_strings(&self) -> Vec<(String, String)> {
        let mut bad = Vec::new();
        for (handle, id) in &self.org {
            // **Derived from the serialised form rather than enumerated.**
            //
            // The list used to be written out field by field, and the field
            // added in the round before this one did not reach it. So did the
            // one before that: the test guarding against the first omission,
            // `every_field_that_reaches_generated_output_is_checked`, is itself
            // a hand-written list and could not see the second.
            //
            // Serialising sees every field, including the ones nobody has added
            // yet, which is the only version of this that does not need somebody
            // to remember. It costs one serialisation per entry on a file read.
            // FIXME: this fails open on a check that exists to refuse hostile
            // strings. An entry whose serialisation fails is skipped silently
            // rather than reported, so a field that could fail to serialise
            // would disable the check for its whole entry. No current field can,
            // and the claim below that the failure surfaces at the write is
            // unverified: nothing in the tree serialises `Workspace` back to
            // TOML yet. Unblocked when the store gains a write path.
            let Ok(value) = toml::Value::try_from(id) else {
                // A value that will not serialise cannot be written to the
                // registry either, so there is nothing here to check. The
                // failure surfaces where the file is written.
                continue;
            };
            walk_for_control_characters(&value, "", &mut |field| {
                bad.push((handle.clone(), field.to_string()));
            });
        }
        bad
    }
}

/// Every string anywhere in a serialised value, with its dotted path, reported
/// when it carries a control character.
///
/// Recursive because a field may be a table or an array, and a check that
/// handled only the flat case would be the same omission one level down.
fn walk_for_control_characters(value: &toml::Value, path: &str, found: &mut impl FnMut(&str)) {
    match value {
        toml::Value::String(s) => {
            if s.chars().any(|c| c.is_control()) {
                found(path);
            }
        }
        toml::Value::Table(t) => {
            for (k, v) in t {
                let next = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                walk_for_control_characters(v, &next, found);
            }
        }
        toml::Value::Array(a) => {
            for v in a {
                walk_for_control_characters(v, path, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"
"#;

    const WITH_ORG: &str = r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"

[paths]
hands = "custom/hands"

[org.op]
role = "king"
handle = "op"

[org.paja]
role = "hand"
staffed = true
handle = "paja"
nickname = "Paja"
git_name = "paja"
git_email = "paja@example.invalid"
workspace = "/tmp/paja"
repos = ["homma"]

[org.rendering]
role = "hand"
handle = "rendering"
domain = "rendering"

[org.proof]
role = "expert"
handle = "proof"
"#;

    #[test]
    fn one_key_is_enough() {
        let w = Workspace::parse(MINIMAL).expect("should parse");
        assert_eq!(
            w.content_repo,
            "git@example.invalid:orgrinrt/clause-dev.git"
        );
        assert!(w.org.is_empty());
    }

    #[test]
    fn every_path_defaults_and_creating_none_of_them_is_valid() {
        let w = Workspace::parse(MINIMAL).unwrap();
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new(".shared/hands")
        );
        assert_eq!(
            w.paths.channels.as_path(),
            std::path::Path::new(".shared/channels")
        );
    }

    #[test]
    fn an_override_replaces_only_what_it_names() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new("custom/hands")
        );
        // The others keep their defaults rather than vanishing.
        assert_eq!(
            w.paths.experts.as_path(),
            std::path::Path::new(".shared/experts")
        );
    }

    #[test]
    fn a_configured_path_that_leaves_the_workspace_is_refused_when_parsed() {
        // The seventh instance of one class: `hands` and `agents` are joined
        // onto the workspace root and were unvalidated strings, so both an
        // escaping and an absolute value wrote a Hand's directories and
        // definitions into an unrelated repository's tree, exit 0.
        for bad in ["../victim/stolen", "/etc/passwd-ish", "a/../../out"] {
            let toml = format!("content_repo = \"local\"\n\n[paths]\nhands = \"{bad}\"\n");
            assert!(
                Workspace::parse(&toml).is_err(),
                "`{bad}` must be refused where somebody can act on it"
            );
        }
    }

    #[test]
    fn a_configured_path_that_stays_inside_is_accepted() {
        // Including one that climbs and comes back, which is contained.
        let toml = "content_repo = \"local\"\n\n[paths]\nhands = \"a/../b/hands\"\n";
        let w = Workspace::parse(toml).expect("contained paths are fine");
        assert_eq!(
            w.paths.hands.as_path(),
            std::path::Path::new("a/../b/hands")
        );
    }

    #[test]
    fn a_consultant_needs_nothing_but_a_role_and_a_handle() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        let proof = &w.org["proof"];
        assert_eq!(proof.role, Role::Expert);
        assert!(proof.workspace.is_none());
        assert!(proof.git_name.is_none());
        // Complete as an entry, and still nothing to stand up, which is a
        // different statement and is what staffing reports.
        assert_eq!(proof.staffing(), Staffing::NoWorkspace);
    }

    #[test]
    fn a_staffed_hand_missing_its_identity_says_which_fields() {
        let mut bare = Identity::new(Role::Hand, "nameless");
        bare.staffed = true;
        assert_eq!(
            bare.staffing(),
            Staffing::Incomplete(vec!["git_name", "git_email", "workspace"])
        );
    }

    #[test]
    fn a_complete_hand_is_staffed() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.org["paja"].staffing(), Staffing::Staffed);
        assert!(w.org["paja"].staffing().can_stand_up());
    }

    #[test]
    fn a_mapped_hand_is_not_reported_as_incomplete() {
        // The whole reason staffing is an enum. A mapped entry lacks exactly
        // the fields an unfinished one lacks, so a list of gaps reports a
        // decision as a mistake, and a registry of a dozen mapped entries
        // reports a dozen broken ones.
        let w = Workspace::parse(WITH_ORG).unwrap();
        let mapped = &w.org["rendering"];
        assert_eq!(mapped.staffing(), Staffing::Mapped);
        assert!(!mapped.staffing().can_stand_up());
    }

    #[test]
    fn an_entry_that_says_nothing_about_staffing_is_mapped_rather_than_broken() {
        // Through TOML, over an entry no other test touches, because the
        // property claimed is serde's default and both a constructor literal
        // and a fixture another test already asserts on would say nothing.
        let w = Workspace::parse(
            r#"
content_repo = "git@example.invalid:orgrinrt/clause-dev.git"

[org.silent]
role = "hand"
handle = "silent"
"#,
        )
        .unwrap();
        assert!(!w.org["silent"].staffed, "the default must be unstaffed");
        assert_eq!(w.org["silent"].staffing(), Staffing::Mapped);
    }

    #[test]
    fn a_role_that_owns_no_workspace_reports_that_rather_than_nothing() {
        // It reported nothing, so a caller checking for gaps found none and
        // stood one up anyway. Under the king's handle that manufactured a
        // dispatchable agent for the human.
        for role in [Role::King, Role::Expert, Role::General] {
            let id = Identity::new(role, "someone");
            assert_eq!(
                id.staffing(),
                Staffing::NoWorkspace,
                "{role:?} must report that it cannot be stood up"
            );
        }
    }

    #[test]
    fn staffing_a_role_that_owns_no_workspace_changes_nothing() {
        // Setting the flag on a King must not manufacture a path to staffing
        // one up, since the role is what decides whether a workspace exists at
        // all and the flag only says whether one is intended.
        let mut king = Identity::new(Role::King, "op");
        king.staffed = true;
        assert_eq!(king.staffing(), Staffing::NoWorkspace);
    }

    #[test]
    fn a_control_character_in_any_registry_string_is_reported() {
        // A newline here becomes an arbitrary key in generated frontmatter.
        let mut w = Workspace::parse(MINIMAL).unwrap();
        let mut id = Identity::new(Role::Hand, "paja");
        id.nickname = Some("Paja\nmemory: project".into());
        w.org.insert("paja".into(), id);
        let bad = w.unsafe_strings();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0], ("paja".to_string(), "nickname".to_string()));
    }

    /// Every free-form string an entry carries, by dotted path.
    ///
    /// **A literal list, on purpose.** The previous version of this test derived
    /// the expected set with the same walker the production code uses, so both
    /// sides of its equality were one computation on one value and it could not
    /// fail: deleting the array arm, so the one array field went unscanned,
    /// left it green. A list is worse to maintain and is the only thing here
    /// that a wrong walker cannot satisfy.
    ///
    /// `role` is absent because it is a closed vocabulary that serialises as a
    /// string and cannot carry a control character.
    const CORRUPTIBLE: &[&str] = &[
        "handle",
        "nickname",
        "full_name",
        "domain",
        "git_name",
        "git_email",
        "committer_email",
        "committer_name",
        "workspace",
        "session",
        "repos",
    ];

    // **Naming, which the literal below cannot force.** A struct literal makes
    // `E0063` demand a value for a new field, and the hostile-value assertion
    // makes that value be one the check should catch. Neither reaches `None`:
    // `skip_serializing_if` removes an unpopulated `Option` from the serialised
    // entry, so a field added as `None` satisfies the compiler and is invisible
    // to everything below.
    //
    // An exhaustive destructuring closes it, and it is kept **beside** the
    // literal rather than instead of it, which is the mistake a previous round
    // made in each direction. There are no `_` patterns: a field can still be
    // classified wrongly on purpose, which is the bar, but not by omission.
    #[test]
    fn every_field_is_classified_as_free_form_or_not() {
        let Identity {
            // Not free-form. A closed vocabulary and a flag; neither can carry a
            // control character.
            role,
            staffed,
            // Free-form. Each must appear in `CORRUPTIBLE`, and each must be
            // populated with a hostile value in the test below.
            handle,
            nickname,
            full_name,
            domain,
            git_name,
            git_email,
            committer_email,
            committer_name,
            workspace,
            session,
            repos,
        } = Identity::new(Role::Hand, "paja");

        // Bound, so deleting a line above without deleting its name here is also
        // a compile error rather than a silent narrowing.
        let not_free_form = [format!("{role:?}"), format!("{staffed:?}")];
        let free_form = [
            ("handle", format!("{handle:?}")),
            ("nickname", format!("{nickname:?}")),
            ("full_name", format!("{full_name:?}")),
            ("domain", format!("{domain:?}")),
            ("git_name", format!("{git_name:?}")),
            ("git_email", format!("{git_email:?}")),
            ("committer_email", format!("{committer_email:?}")),
            ("committer_name", format!("{committer_name:?}")),
            ("workspace", format!("{workspace:?}")),
            ("session", format!("{session:?}")),
            ("repos", format!("{repos:?}")),
        ];
        assert_eq!(not_free_form.len(), 2);

        let named: Vec<&str> = free_form.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            named, CORRUPTIBLE,
            "a field classified free-form here must appear in CORRUPTIBLE, which is \
             what the hostile-entry test asserts production reports"
        );
    }

    #[test]
    fn every_free_form_string_is_reported_when_it_carries_a_control_character() {
        // **A struct literal, and that is the gate.** Written as
        // `Identity::new` plus assignments, a field added later is simply not
        // populated, `skip_serializing_if` keeps it out of the serialised value,
        // and nothing here or in production ever sees it. A literal is
        // exhaustive: `E0063` refuses to compile until the new field is given a
        // value, that value carries a control character like every other, and
        // production then has to report it or the assertion below fails.
        //
        // So adding a string to `Identity` cannot reach the registry unguarded
        // without somebody deliberately writing a benign value here.
        let bad = Identity {
            role: Role::Hand,
            staffed: false,
            handle: "pa\nja".into(),
            nickname: Some("a\nb".into()),
            full_name: Some("a\nb".into()),
            domain: Some("a\nb".into()),
            git_name: Some("a\nb".into()),
            git_email: Some("a\nb".into()),
            committer_email: Some("a\nb".into()),
            committer_name: Some("a\nb".into()),
            workspace: Some("a\nb".into()),
            session: Some("a\nb".into()),
            repos: vec!["a\nb".into()],
        };

        // **And every string in it is actually hostile.** `E0063` forces a new
        // field to be given a value; it cannot force that value to be one the
        // check should catch, so a benign one would satisfy the compiler and
        // leave the field unguarded. This closes that: serialise the entry and
        // require that every string in it carries a control character, so a
        // benign value fails here rather than silently passing below.
        //
        // `role` is the one exception, being a closed vocabulary that
        // serialises as a string and cannot carry one.
        {
            fn every_string(v: &toml::Value, at: &str, out: &mut Vec<(String, String)>) {
                match v {
                    toml::Value::String(s) => out.push((at.to_string(), s.clone())),
                    toml::Value::Table(t) => {
                        for (k, v) in t {
                            let next = if at.is_empty() {
                                k.clone()
                            } else {
                                format!("{at}.{k}")
                            };
                            every_string(v, &next, out);
                        }
                    }
                    toml::Value::Array(a) => {
                        for v in a {
                            every_string(v, at, out);
                        }
                    }
                    _ => {}
                }
            }
            let mut all = Vec::new();
            every_string(
                &toml::Value::try_from(&bad).expect("an entry serialises"),
                "",
                &mut all,
            );
            for (field, value) in all {
                if field == "role" {
                    continue;
                }
                assert!(
                    value.chars().any(|c| c.is_control()),
                    "`{field}` was given a benign value in the hostile entry, so this \
                     test cannot tell whether the check covers it"
                );
            }
        }

        let mut ws = Workspace::parse(MINIMAL).expect("the minimal fixture parses");
        ws.org.insert("paja".into(), bad);

        let reported: std::collections::BTreeSet<String> =
            ws.unsafe_strings().into_iter().map(|(_, f)| f).collect();
        let want: std::collections::BTreeSet<String> =
            CORRUPTIBLE.iter().map(|s| s.to_string()).collect();

        assert_eq!(
            reported, want,
            "every free-form string carrying a control character must be reported, \
             and nothing else"
        );
    }

    #[test]
    fn a_clean_registry_reports_nothing_unsafe() {
        assert!(Workspace::parse(WITH_ORG)
            .unwrap()
            .unsafe_strings()
            .is_empty());
    }

    #[test]
    fn the_registry_answers_what_role_a_handle_holds() {
        let w = Workspace::parse(WITH_ORG).unwrap();
        assert_eq!(w.role_of("op"), Some(Role::King));
        assert_eq!(w.role_of("paja"), Some(Role::Hand));
        assert_eq!(w.role_of("stranger"), None);
    }
}
